/** Deterministic local Three.js -> Edge -> FFmpeg movie exporter. */
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const cache=path.join(root,'target','promo-fluent');
const output=path.join(root,'docs','promo','fluent');
const bundle=path.join(process.env.USERPROFILE??'','.cache','codex-runtimes','codex-primary-runtime','dependencies');
const python=process.env.CODEX_PYTHON??(fs.existsSync(path.join(bundle,'python','python.exe'))?path.join(bundle,'python','python.exe'):'python');
const require=createRequire(import.meta.url);
const {chromium}=require(require.resolve('playwright',{paths:[path.join(root,'target','promo-web','node_modules'),path.join(bundle,'node','node_modules')]}));
const previewOnly=process.argv.includes('--preview-only');
const fps=30,seconds=60;
fs.mkdirSync(cache,{recursive:true});fs.mkdirSync(output,{recursive:true});
function pythonTask(...args){const r=spawnSync(python,[path.join(root,'scripts','promo-fluent-media.py'),...args],{cwd:root,encoding:'utf8',maxBuffer:2e6});if(r.status!==0)throw Error(r.stderr||r.stdout);if(r.stdout.trim())console.log(r.stdout.trim());}
pythonTask('--prepare');
const executable=spawnSync(python,['-c','import sys;sys.path.insert(0,"target/promo-deps");import imageio_ffmpeg;print(imageio_ffmpeg.get_ffmpeg_exe())'],{cwd:root,encoding:'utf8'});
if(executable.status!==0)throw Error(executable.stderr);
const ffmpeg=executable.stdout.trim();
const contentTypes={'.html':'text/html; charset=utf-8','.mjs':'text/javascript; charset=utf-8','.js':'text/javascript; charset=utf-8','.png':'image/png','.svg':'image/svg+xml','.jpg':'image/jpeg'};
const server=http.createServer((request,response)=>{
  try{
    const pathname=decodeURIComponent(new URL(request.url,'http://localhost').pathname);
    const target=path.resolve(root,'.'+pathname);
    if(!target.startsWith(root+path.sep)||!fs.statSync(target).isFile()){response.writeHead(404).end();return;}
    response.writeHead(200,{'Content-Type':contentTypes[path.extname(target)]??'application/octet-stream','Cache-Control':'no-store'});
    fs.createReadStream(target).pipe(response);
  }catch{response.writeHead(404).end();}
});
server.listen(0,'127.0.0.1');await once(server,'listening');
let browser,encoder;
try{
  browser=await chromium.launch({channel:'msedge',headless:true,args:['--enable-webgl','--enable-unsafe-swiftshader','--hide-scrollbars','--disable-background-timer-throttling']});
  const page=await browser.newPage({viewport:{width:1920,height:1080},deviceScaleFactor:1});
  const errors=[];page.on('pageerror',error=>errors.push(String(error)));
  await page.goto(`http://127.0.0.1:${server.address().port}/docs/promo/fluent/scene.html`,{waitUntil:'networkidle'});
  await page.waitForFunction(()=>window.ready===true,{},{timeout:60000});
  if(errors.length)throw Error(errors.join('\n'));
  const shots=await page.evaluate(()=>window.shots);
  fs.writeFileSync(path.join(output,'storyboard.json'),JSON.stringify(shots,null,2)+'\n');
  const times=[3.9,8.3,13.7,20.3,26.7,33.8,40.7,47.3,52.2,57.3];
  for(let i=0;i<times.length;i++){
    await page.evaluate(t=>window.renderAt(t),times[i]);
    await page.screenshot({path:path.join(cache,`preview-${i}.jpg`),type:'jpeg',quality:95});
  }
  pythonTask('--previews');
  if(!previewOnly){
    const partial=path.join(cache,'NorthStar-Fluent-60s.partial.mp4');
    const log=fs.openSync(path.join(cache,'encode.log'),'w');
    encoder=spawn(ffmpeg,['-y','-hide_banner','-loglevel','warning','-f','image2pipe','-vcodec','mjpeg','-r',String(fps),'-i','pipe:0',
      '-i',path.join(cache,'score.wav'),'-map','0:v:0','-map','1:a:0','-c:v','libx264','-preset','fast','-crf','18',
      '-pix_fmt','yuv420p','-profile:v','high','-level:v','4.1','-threads','6','-c:a','aac','-b:a','192k','-ar','48000',
      '-af','loudnorm=I=-16:TP=-1.5:LRA=9','-t','60','-movflags','+faststart','-metadata','title=NorthStar | 下一站，你的世界 · Fluent',partial],
    {cwd:root,stdio:['pipe','ignore',log]});
    const encoderDone=new Promise((resolve,reject)=>{encoder.on('error',reject);encoder.on('close',code=>code===0?resolve():reject(Error(`FFmpeg exited ${code}`)));});
    // Register immediately: a codec startup error must not become an unhandled rejection.
    encoderDone.catch(()=>{});
    for(let frame=0;frame<seconds*fps;frame++){
      await page.evaluate(t=>window.renderAt(t),frame/fps);
      const jpeg=await page.screenshot({type:'jpeg',quality:94});
      if(!encoder.stdin.write(jpeg))await once(encoder.stdin,'drain');
      if(frame%150===0)console.log(`Rendered ${String(frame/fps).padStart(2,'0')}/60 seconds`);
    }
    encoder.stdin.end();await encoderDone;fs.closeSync(log);
    if(errors.length)throw Error(errors.join('\n'));
    fs.renameSync(partial,path.join(output,'NorthStar-Fluent-60s-1080p.mp4'));
    pythonTask('--verify');
  }
  console.log(`Deliverables: ${output}`);
}finally{
  if(encoder&&encoder.exitCode===null)encoder.kill();
  if(browser)await browser.close();server.close();
}
