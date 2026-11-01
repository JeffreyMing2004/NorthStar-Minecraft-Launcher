use super::*;
use reqwest::{
    StatusCode,
    header::{CONTENT_RANGE, ETAG, IF_RANGE, LAST_MODIFIED, RANGE},
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Resume {
    url: String,
    sha1: String,
    size: Option<u64>,
    validator: Option<String>,
}

#[cfg(test)]
#[path = "resume_tests.rs"]
mod tests;
impl Downloader {
    pub(super) fn resumable(&self, job: &Job, candidates: &[String]) -> Result<()> {
        validate_hash(&job.sha1)?;
        self.check_cancelled()?;
        if verified(&job.path, &job.sha1, job.size)? {
            return Ok(());
        }
        let parent = job.path.parent().ok_or("无效下载路径")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let part = job.path.with_file_name(format!(
            "{}.part",
            job.path.file_name().unwrap().to_string_lossy()
        ));
        let meta = part.with_file_name(format!(
            "{}.json",
            part.file_name().unwrap().to_string_lossy()
        ));
        for path in [parent, &job.path, &part, &meta] {
            crate::instances::reject_link(path)?;
        }
        let mut errors = Vec::new();
        for url in candidates {
            for _ in 0..2 {
                self.check_cancelled()?;
                let stored: Option<Resume> = fs::metadata(&meta)
                    .ok()
                    .filter(|m| m.len() < 16384)
                    .and_then(|_| fs::read(&meta).ok())
                    .and_then(|b| serde_json::from_slice(&b).ok());
                let mut offset = match &stored {
                    Some(m)
                        if m.url == *url
                            && m.sha1.eq_ignore_ascii_case(&job.sha1)
                            && m.size == job.size =>
                    {
                        fs::metadata(&part).map(|m| m.len()).unwrap_or(0)
                    }
                    _ => 0,
                };
                if offset > job.size.unwrap_or(2 * 1024 * 1024 * 1024) {
                    offset = 0;
                }
                if offset > 0 && verified(&part, &job.sha1, job.size)? {
                    replace_file(&part, &job.path)?;
                    let _ = fs::remove_file(&meta);
                    return Ok(());
                }
                let result = (|| -> Result<()> {
                    let mut request = self
                        .client
                        .get(url)
                        .header(reqwest::header::ACCEPT_ENCODING, "identity");
                    if offset > 0 {
                        request = request.header(RANGE, format!("bytes={offset}-"));
                        if let Some(v) = stored.as_ref().and_then(|m| m.validator.as_ref()) {
                            request = request.header(IF_RANGE, v);
                        }
                    }
                    let mut response = request.send().map_err(network_error)?;
                    if response.status() == StatusCode::RANGE_NOT_SATISFIABLE {
                        let _ = fs::remove_file(&part);
                        let _ = fs::remove_file(&meta);
                        return Err("服务器拒绝续传范围，将重新下载。".into());
                    }
                    response = response.error_for_status().map_err(network_error)?;
                    if response.status() == StatusCode::PARTIAL_CONTENT {
                        let range = response
                            .headers()
                            .get(CONTENT_RANGE)
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("");
                        let parsed = range
                            .strip_prefix("bytes ")
                            .and_then(|v| v.split_once('/'))
                            .and_then(|(bounds, total)| {
                                bounds.split_once('-').and_then(|(a, b)| {
                                    Some((
                                        a.parse::<u64>().ok()?,
                                        b.parse::<u64>().ok()?,
                                        total.parse::<u64>().ok()?,
                                    ))
                                })
                            });
                        if !parsed.is_some_and(|(start, end, total)| {
                            start == offset
                                && end >= start
                                && end < total
                                && job.size.is_none_or(|s| s == total)
                        }) {
                            let _ = fs::remove_file(&part);
                            let _ = fs::remove_file(&meta);
                            return Err("续传响应范围与文件不匹配。".into());
                        }
                    } else {
                        offset = 0;
                        if job
                            .size
                            .zip(response.content_length())
                            .is_some_and(|(expected, actual)| expected != actual)
                        {
                            let _ = fs::remove_file(&part);
                            let _ = fs::remove_file(&meta);
                            return Err("文件长度与元数据不符。".into());
                        }
                    }
                    let validator = response
                        .headers()
                        .get(ETAG)
                        .and_then(|v| v.to_str().ok())
                        .filter(|v| !v.starts_with("W/"))
                        .or_else(|| {
                            response
                                .headers()
                                .get(LAST_MODIFIED)
                                .and_then(|v| v.to_str().ok())
                        })
                        .map(str::to_owned);
                    super::write_json(
                        &meta,
                        &Resume {
                            url: url.clone(),
                            sha1: job.sha1.clone(),
                            size: job.size,
                            validator,
                        },
                    )?;
                    let mut hash = Sha1::new();
                    if offset > 0 {
                        let mut input = File::open(&part).map_err(|e| e.to_string())?;
                        let mut buffer = [0; 65536];
                        loop {
                            self.check_cancelled()?;
                            let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
                            if n == 0 {
                                break;
                            }
                            hash.update(&buffer[..n]);
                        }
                    }
                    let mut file = fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .append(offset > 0)
                        .truncate(offset == 0)
                        .open(&part)
                        .map_err(|e| e.to_string())?;
                    let mut size = offset;
                    let mut buffer = [0; 65536];
                    loop {
                        self.check_cancelled()?;
                        let n = response.read(&mut buffer).map_err(|e| e.to_string())?;
                        if n == 0 {
                            break;
                        }
                        size += n as u64;
                        if size > job.size.unwrap_or(2 * 1024 * 1024 * 1024) {
                            return Err("文件长度超出元数据记录。".into());
                        }
                        file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
                        hash.update(&buffer[..n]);
                        self.update(
                            &format!("下载 {}", job.path.file_name().unwrap().to_string_lossy()),
                            n as u64,
                            false,
                        );
                    }
                    file.sync_all().map_err(|e| e.to_string())?;
                    drop(file);
                    if job.size.is_some_and(|s| s != size) {
                        return Err("下载中断，已保留可续传的数据。".into());
                    }
                    if format!("{:x}", hash.finalize()) != job.sha1.to_ascii_lowercase() {
                        let _ = fs::remove_file(&part);
                        let _ = fs::remove_file(&meta);
                        return Err("SHA-1 校验失败。".into());
                    }
                    self.check_cancelled()?;
                    replace_file(&part, &job.path)?;
                    let _ = fs::remove_file(&meta);
                    Ok(())
                })();
                match result {
                    Ok(()) => return Ok(()),
                    Err(e) => {
                        self.check_cancelled()?;
                        errors.push(format!("{url}: {e}"));
                    }
                }
            }
        }
        Err(format!(
            "下载失败：{}\n{}",
            job.path.display(),
            errors.join("\n")
        ))
    }
}
