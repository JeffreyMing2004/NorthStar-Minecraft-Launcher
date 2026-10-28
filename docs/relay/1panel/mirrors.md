# 腾讯云和大学镜像配置

部署包默认使用腾讯云 Debian 软件源、南京大学 Cargo 稀疏索引。项目根目录的 `.cargo/config.toml` 也使用南京大学源，只影响本项目，不修改用户全局配置或 `Cargo.lock`。

## 1. 腾讯云 Docker 加速

在腾讯云服务器的 1Panel Docker 配置中，将以下地址加入镜像加速列表：

```text
https://mirror.ccs.tencentyun.com
```

这是腾讯云内网加速地址，适用于腾讯云 CVM/Lighthouse。它是 Docker 的 `registry-mirrors` 配置，Dockerfile 仍保留 `rust:1.99.1-slim-bookworm` 和 `debian:bookworm-slim` 等原镜像名。

包内的 `docker-daemon.tencent.example.json` 是配置片段。手工配置 `/etc/docker/daemon.json` 时，将该地址合并进现有 `registry-mirrors` 数组，保留其他配置项，不要用示例覆盖整个文件。保存和重启 Docker 会影响运行中的容器，请安排维护时操作。

在服务器检查实际生效和镜像拉取情况：

```sh
docker info --format '{{json .RegistryConfig.Mirrors}}'
docker pull rust:1.99.1-slim-bookworm
docker pull debian:bookworm-slim
```

这两次拉取成功后再构建联机服务。内网镜像的实际可用性须在腾讯云服务器验证，本地公网测试不能代替。

## 2. 腾讯云 APT 软件源

构建和运行阶段默认把 Debian 的普通、安全更新源替换到：

```text
https://mirrors.cloud.tencent.com/debian
https://mirrors.cloud.tencent.com/debian-security
```

`configure-apt.sh` 同时处理传统 `.list` 与 deb822 `.sources` 格式，保留版本代号、组件和 Debian 签名配置。运行阶段先从构建阶段复制 CA 证书，再进行第一次 HTTPS APT 请求。

## 3. Cargo 大学镜像

默认南京大学源：

```toml
[source.crates-io]
replace-with = "nju"

[source.nju]
registry = "sparse+https://mirror.nju.edu.cn/crates.io-index/"
```

交大源作为可选项，切换示例：

```sh
NSCL_CARGO_MIRROR=sjtu docker compose build relay
```

镜像可能存在同步延迟；若提示锁定版本不存在，应切换源，不要为换源修改依赖版本或 `Cargo.lock`。本次实际检查南京大学源已能下载项目依赖并完成 `cargo check --locked --no-default-features --bin nscl-relay`。

## 4. 构建与切回官方源

使用默认的腾讯云 APT + 南京大学 Cargo：

```sh
docker compose build relay
docker compose up -d
```

临时恢复容器构建中的官方 Debian 和 Cargo 源：

```sh
NSCL_DEBIAN_MIRROR=official NSCL_CARGO_MIRROR=official docker compose build relay
docker compose up -d
```

这些构建参数不会修改宿主机的 APT 或全局 Cargo 配置。Docker 镜像拉取加速由 Docker 守护进程单独管理；如需恢复官方 Docker Hub，在 1Panel 中移除腾讯云加速条目。若使用独立的 `docker-container` BuildKit builder，还需在该 builder 的配置中设置镜像加速。

也可通过 `NSCL_RUST_IMAGE`、`NSCL_RUNTIME_IMAGE` 指定已有的自建镜像地址，保持与 Debian bookworm 兼容。普通 1Panel 部署无需修改这两个参数。

参考：[腾讯云软件源及 Docker 加速说明](https://cloud.tencent.com/document/product/213/8623)、[南京大学镜像站](https://mirror.nju.edu.cn/crates.io-index/)、[Cargo 源替换](https://doc.rust-lang.org/cargo/reference/source-replacement.html)、[BuildKit 镜像加速配置](https://docs.docker.com/build/buildkit/configure/#registry-mirror)。
