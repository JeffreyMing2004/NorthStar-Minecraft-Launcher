import * as THREE from 'three';
import { RoomEnvironment } from 'three/addons/environments/RoomEnvironment.js';
import { RoundedBoxGeometry } from 'three/addons/geometries/RoundedBoxGeometry.js';

// Every position is a function of the requested time: no wall clock, CSS
// transitions, random frame state, or requestAnimationFrame in export mode.
const renderer = new THREE.WebGLRenderer({ antialias:true, alpha:true, preserveDrawingBuffer:true });
renderer.setSize(1920,1080);
renderer.setPixelRatio(1);
renderer.outputColorSpace = THREE.SRGBColorSpace;
renderer.toneMapping = THREE.ACESFilmicToneMapping;
renderer.toneMappingExposure = 1.15;
renderer.setClearColor(0,0);
document.querySelector('#stage').appendChild(renderer.domElement);
const scene = new THREE.Scene();
const camera = new THREE.PerspectiveCamera(39,1920/1080,.1,120);
const pmrem = new THREE.PMREMGenerator(renderer);
const room = new RoomEnvironment();
const env = pmrem.fromScene(room,.04);
scene.environment = env.texture;
room.dispose(); pmrem.dispose();
scene.add(new THREE.HemisphereLight('#f9ffff','#668c73',2.3));
for (const [color,intensity,x,y,z] of [
  ['#ffffff',55,-4,6,5],['#c7ffe2',38,6,1,4],['#dce7ff',45,-5,-1,1],
]) {
  const light = new THREE.PointLight(color,intensity,35,2);
  light.position.set(x,y,z);scene.add(light);
}
const all = [];
const register = object => { scene.add(object);all.push(object);return object; };
const clamp = n => Math.max(0,Math.min(1,n));
const ease = n => { n=clamp(n);return n*n*n*(n*(n*6-15)+10); };
const out = n => 1-(1-clamp(n))**4;
const mix = THREE.MathUtils.lerp;
const $ = id => document.getElementById(id);
const textureLoader = new THREE.TextureLoader();

function physical(color,options={}) {
  return new THREE.MeshPhysicalMaterial({color,roughness:.27,metalness:.12,
    clearcoat:1,clearcoatRoughness:.15,envMapIntensity:1.1,...options});
}
function roundedShape(w,h,r) {
  const s = new THREE.Shape();
  const x=-w/2,y=-h/2;
  s.moveTo(x+r,y);s.lineTo(x+w-r,y);s.quadraticCurveTo(x+w,y,x+w,y+r);
  s.lineTo(x+w,y+h-r);s.quadraticCurveTo(x+w,y+h,x+w-r,y+h);
  s.lineTo(x+r,y+h);s.quadraticCurveTo(x,y+h,x,y+h-r);
  s.lineTo(x,y+r);s.quadraticCurveTo(x,y,x+r,y);return s;
}
function softShadow(w,h) {
  const c=document.createElement('canvas');c.width=384;c.height=256;
  const ctx=c.getContext('2d');
  ctx.translate(192,128);ctx.scale(192,128);
  const g=ctx.createRadialGradient(0,0,.02,0,0,1);
  g.addColorStop(0,'rgba(26,67,41,.26)');g.addColorStop(.5,'rgba(26,67,41,.11)');g.addColorStop(1,'rgba(26,67,41,0)');
  ctx.fillStyle=g;ctx.fillRect(-1,-1,2,2);
  const material=new THREE.MeshBasicMaterial({map:new THREE.CanvasTexture(c),transparent:true,depthWrite:false,toneMapped:false});
  return new THREE.Mesh(new THREE.PlaneGeometry(w,h),material);
}
function makeLogo() {
  const group=register(new THREE.Group());
  const tile=new THREE.Mesh(new RoundedBoxGeometry(2.6,2.6,.48,7,.35),physical('#173d2c',{metalness:.2,roughness:.24}));
  group.add(tile);
  const s=new THREE.Shape();
  [[0,1.01],[.25,.25],[1.01,0],[.25,-.25],[0,-1.01],[-.25,-.25],[-1.01,0],[-.25,.25]].forEach(([x,y],i)=>i?s.lineTo(x,y):s.moveTo(x,y));s.closePath();
  const geometry=new THREE.ExtrudeGeometry(s,{depth:.08,bevelEnabled:true,bevelSize:.028,bevelThickness:.025,bevelSegments:3,steps:1});
  const icon=new THREE.Mesh(geometry,physical('#c2fbd7',{metalness:.25,roughness:.2,emissive:'#37624a',emissiveIntensity:.08}));
  icon.position.z=.245;group.add(icon);
  const shadow=softShadow(6.3,4.9);shadow.position.set(.25,-.3,-.5);group.add(shadow);
  return group;
}
const logo=makeLogo();

function makeBloom() {
  const group=register(new THREE.Group());
  const colors=['#66ba8e','#9bd6b3','#d6eedd','#9acfb6','#f0f6e9'];
  for(let layer=0;layer<5;layer++) {
    const radial=160,across=24,positions=[],uvs=[],indices=[];
    for(let a=0;a<=radial;a++)for(let b=0;b<=across;b++) {
      const u=a/radial*Math.PI*2,v=b/across,phase=layer*.67;
      const radius=.30+v*1.75+.17*Math.sin(u*7+phase)*v*v;
      positions.push(radius*Math.cos(u),radius*Math.sin(u),
        .95*(1-v)**2+.27*Math.cos(7*u+phase)*v*v-.22*layer);
      uvs.push(a/radial,v);
      if(a<radial&&b<across){const i=a*(across+1)+b;indices.push(i,i+1,i+across+1,i+1,i+across+2,i+across+1);}
    }
    const g=new THREE.BufferGeometry();
    g.setAttribute('position',new THREE.Float32BufferAttribute(positions,3));
    g.setAttribute('uv',new THREE.Float32BufferAttribute(uvs,2));g.setIndex(indices);g.computeVertexNormals();
    const mat=physical(colors[layer],{side:THREE.DoubleSide,roughness:.3,metalness:.23,
      sheen:.75,sheenColor:new THREE.Color('#efffec'),sheenRoughness:.45});
    const mesh=new THREE.Mesh(g,mat);
    const size=.57+layer*.24;mesh.scale.setScalar(size);mesh.rotation.z=layer*.24;
    mesh.userData.baseScale=size;group.add(mesh);
  }
  const shadow=softShadow(12,9);shadow.position.set(.4,-.5,-2);group.add(shadow);
  return group;
}
const bloom=makeBloom();

async function screen(name,width=9) {
  const image=await new Promise((resolve,reject)=>{const i=new Image();i.onload=()=>resolve(i);i.onerror=reject;i.src='/docs/images/'+name;});
  const c=document.createElement('canvas');c.width=1200;c.height=760;
  const ctx=c.getContext('2d');ctx.beginPath();ctx.roundRect(0,0,1200,760,20);ctx.clip();ctx.drawImage(image,0,0,1200,760);
  const tex=new THREE.CanvasTexture(c);tex.colorSpace=THREE.SRGBColorSpace;tex.anisotropy=renderer.capabilities.getMaxAnisotropy();
  const height=width*760/1200,group=register(new THREE.Group());
  const backing=new THREE.Mesh(new THREE.ExtrudeGeometry(roundedShape(width+.09,height+.09,.14),
    {depth:.09,bevelEnabled:true,bevelSize:.025,bevelThickness:.02,bevelSegments:3,steps:1}),physical('#e8f5ed',{roughness:.22,metalness:.4}));
  backing.position.z=-.13;group.add(backing);
  const pane=new THREE.Mesh(new THREE.PlaneGeometry(width,height),new THREE.MeshBasicMaterial({map:tex,transparent:true,alphaTest:.01,toneMapped:false}));
  pane.position.z=.02;group.add(pane);
  const shadow=softShadow(width*1.35,height*1.6);shadow.position.set(.18,-.22,-.25);group.add(shadow);
  return group;
}
const [home,downloads,instances,mods,backHome,backDownload]=await Promise.all([
  screen('home.png',9.4),screen('downloads.png',8.5),screen('instances.png',8.7),
  screen('instance-files.png',8.1),screen('home.png',8.4),screen('downloads.png',8.4),
]);

function card(label,detail='',width=1.7,height=1.5,variant=0) {
  const group=register(new THREE.Group());
  const colors=['#f0f8f3','#cbe9d5','#e9eff7','#d4eddc'];
  const tile=new THREE.Mesh(new RoundedBoxGeometry(width,height,.16,5,.12),
    physical(colors[variant%colors.length],{roughness:.3,metalness:.05}));group.add(tile);
  const c=document.createElement('canvas');c.width=700;c.height=Math.round(700*height/width);
  const ctx=c.getContext('2d');ctx.clearRect(0,0,c.width,c.height);ctx.textAlign='center';
  ctx.fillStyle='#244f3a';ctx.font=`500 ${label.length>7?61:82}px "Microsoft YaHei",sans-serif`;
  ctx.fillText(label,350,c.height*(detail?.46:.55));if(detail){ctx.fillStyle='#6a8776';ctx.font='36px "Microsoft YaHei",sans-serif';ctx.fillText(detail,350,c.height*.69);}
  const texture=new THREE.CanvasTexture(c);texture.colorSpace=THREE.SRGBColorSpace;
  const text=new THREE.Mesh(new THREE.PlaneGeometry(width*.94,height*.94),new THREE.MeshBasicMaterial({map:texture,transparent:true,toneMapped:false,depthWrite:false}));
  text.position.z=.09;group.add(text);
  const shadow=softShadow(width*1.7,height*1.9);shadow.position.set(.12,-.15,-.17);group.add(shadow);
  return group;
}
const loaders=['Forge','Fabric','NeoForge','Quilt'].map((n,i)=>card(n,'',1.55,1.32,i));
const content=[card('MOD','模组',2.1,1.65,1),card('.mrpack','整合包',2.1,1.65,0),card('FX','光影与资源包',2.1,1.65,2)];
const prep=[card('Java','自动匹配',3.2,2.45,1),card('界面主题','自选风格',3.2,2.45,0),card('Rust','原生构建',3.2,2.45,2)];

const network=register(new THREE.Group());
const nodes=[];
for(let i=0;i<3;i++) {
  const group=new THREE.Group();
  const ball=new THREE.Mesh(new THREE.SphereGeometry(i? .64:.88,48,32),physical(i===1?'#c0d8eb':'#8ed8ac',
    {roughness:.13,metalness:.21,clearcoat:1}));group.add(ball);
  const ring=new THREE.Mesh(new THREE.TorusGeometry(i?.82:1.08,.027,12,80),physical('#e4fff0',{metalness:.45}));
  ring.rotation.x=.22;group.add(ring);network.add(group);nodes.push(group);
}
const nodePositions=[new THREE.Vector3(0,-.7,.7),new THREE.Vector3(-3.5,-.1,-.4),new THREE.Vector3(3.5,.15,-.5)];
nodes.forEach((n,i)=>n.position.copy(nodePositions[i]));
const trails=[];
for(let i=1;i<3;i++) {
  const curve=new THREE.CatmullRomCurve3([nodePositions[0],new THREE.Vector3(nodePositions[i].x*.48,1.4,-.55),nodePositions[i]]);
  const tube=new THREE.Mesh(new THREE.TubeGeometry(curve,80,.022,8,false),physical('#7bbd96',{emissive:'#7bbd96',emissiveIntensity:.35}));
  network.add(tube);
  const particle=new THREE.Mesh(new THREE.SphereGeometry(.09,16,12),new THREE.MeshBasicMaterial({color:'#f8fff7'}));
  network.add(particle);trails.push({curve,particle});
}
const networkLabels=[card('你的世界','创建房间',2.1,1.1,1),card('朋友加入','分享冒险',2.1,1.1,0),card('一起出发','房间环境检查',2.25,1.1,2)];

const shots=[
  {start:0,end:6,title:'让热爱，\n有个新起点。',eyebrow:'A NEW BEGINNING',detail:'',caption:''},
  {start:6,end:10,title:'遇见 NorthStar。',eyebrow:'INTRODUCING',detail:'为下一次冒险而来。',caption:''},
  {start:10,end:17,title:'一个入口。无限可能。',eyebrow:'FEEL AT HOME',detail:'从选择实例，到启动游戏。',caption:''},
  {start:17,end:23,title:'想玩的版本，轻松就位。',eyebrow:'READY FOR YOUR NEXT',detail:'游戏与加载器，从这里开始。',caption:'安装 · 校验 · 修复'},
  {start:23,end:30,title:'一个世界，一份专属。',eyebrow:'A SPACE FOR EVERY WORLD',detail:'',caption:'独立实例 · 内容备份 · 历史恢复 · 实例克隆'},
  {start:30,end:37,title:'为灵感，留出空间。',eyebrow:'MAKE ROOM FOR MORE',detail:'',caption:'Modrinth 模组 · 资源包 · 光影 · 整合包导入导出'},
  {start:37,end:44,title:'让距离，近一点。',eyebrow:'CLOSER, TOGETHER',detail:'',caption:'局域网发现 · 房间环境检查 · 隧道联机  /  联机功能示意'},
  {start:44,end:50,title:'准备妥当。只管出发。',eyebrow:'LESS TO PREPARE. MORE TO EXPLORE.',detail:'自动匹配运行环境，自选界面主题。',caption:''},
  {start:50,end:54,title:'你的玩法。\n你的世界。',eyebrow:'MADE FOR WHAT YOU LOVE',detail:'',caption:''},
  {start:54,end:60,title:'',eyebrow:'',detail:'',caption:''},
];
window.shots=shots;
let active=-1;

function show(object,x,y,z,rx=0,ry=0,rz=0,s=1) {
  object.visible=true;object.position.set(x,y,z);object.rotation.set(rx,ry,rz);object.scale.setScalar(s);
}

window.renderAt=(time)=>{
  const t=Math.max(0,Math.min(time,59.999));
  const index=shots.findIndex(s=>t>=s.start&&t<s.end),shot=shots[index];
  const local=t-shot.start,p=local/(shot.end-shot.start),enter=out(local/1.35);
  if(index!==active){
    $('headline').textContent=shot.title;$('eyebrow').textContent=shot.eyebrow;
    $('detail').textContent=shot.detail;$('caption').textContent=shot.caption;active=index;
  }
  all.forEach(o=>o.visible=false);
  camera.position.set(0,.15,11.7);camera.lookAt(0,0,0);
  $('copy').style.cssText='';$('headline').style.cssText='';$('detail').style.cssText='';
  $('eyebrow').style.cssText='';$('caption').style.cssText='';
  $('copy').style.opacity=String(out((local-.25)/.9));
  $('copy').style.transform=`translateY(${mix(32,0,out((local-.25)/1.1))}px)`;
  $('caption').style.opacity=String(out((local-1)/.8));
  $('brand').style.opacity=index===9?'0':String(index===0?.0:.72);
  $('closing').style.opacity='0';
  $('backdrop').style.background='radial-gradient(ellipse at 65% 45%,#d1ecd9 0%,#e9f5ef 42%,#f5f5f3 85%)';
  $('copy').style.color='#19382e';
  const drift=Math.sin(local*.5)*.035;
  if(index===0){
    camera.position.set(mix(.5,0,ease(p)),mix(.1,.4,ease(p)),mix(5.1,10.8,ease(p)));camera.lookAt(.4,-.1,0);
    show(bloom,mix(.7,2.75,ease(p)),-.15,-.5,.3,.28,mix(-.42,.2,ease(p)),1.3);
    bloom.children.slice(0,5).forEach((m,i)=>m.scale.setScalar(m.userData.baseScale*(.7+.3*out((local-i*.2)/3))));
    $('copy').style.cssText=`top:326px;left:150px;right:auto;text-align:left;opacity:${out((local-1.1)/1.3)};transform:translateY(${mix(35,0,enter)}px)`;
    $('headline').style.fontSize='96px';$('headline').style.lineHeight='1.48';
    $('eyebrow').style.marginBottom='34px';
  } else if(index===1){
    show(logo,0,mix(-.9,-.5,enter),.6,mix(.35,-.05,enter)+drift,mix(-.65,.12,enter),-.045,1.12);
    show(bloom,0,-1.1,-3.9,.45,.12,-.3+local*.035,1.35);
    bloom.children.slice(0,5).forEach(m=>m.scale.setScalar(m.userData.baseScale));
    camera.position.z=mix(12.8,10.8,ease(p));
  } else if(index===2){
    show(home,mix(1.4,0,enter),mix(-1.55,-1.09,enter),mix(-1,.3,enter),mix(.14,-.035,enter)+drift*.2,mix(-.32,.018,enter),mix(.035,-.012,ease(p)),mix(.79,.83,ease(p)));
    camera.position.z=mix(12.1,11.25,ease(p));
  } else if(index===3){
    show(downloads,mix(-2.5,-1.45,enter),-1.05,mix(-.9,.1,enter),.025+drift*.2,mix(.24,.065,enter),-.014,.90);
    loaders.forEach((card,i)=>{
      const q=out((local-.35-i*.12)/1.1),col=i%2,row=Math.floor(i/2);
      show(card,3.65+col*1.73+mix(2,0,q),.35-row*1.7,.55+Math.sin(local*.7+i)*.08,
        -.1,mix(-.5,-.1,q),Math.sin(local*.5+i)*.025,q);
    });
  } else if(index===4){
    const spread=ease(clamp((local-.2)/2));
    show(backHome,-1.8*spread,-.35,-1.5,-.025,.11,-.075*spread,.85);
    show(backDownload,1.7*spread,-.3,-1.8,.04,-.13,.075*spread,.85);
    show(instances,0,mix(-1.6,-.98,enter),.65,-.025+drift*.15,.025*Math.sin(local*.4),-.008,.82);
  } else if(index===5){
    show(mods,1.15,mix(-1.5,-.9,enter),.2,.025,mix(-.23,-.06,enter),.014,.90);
    content.forEach((card,i)=>{const q=out((local-.3-i*.15)/1.15);
      show(card,-4.9+Math.sin(local*.6+i)*.12,1.2-i*1.82+mix(1.2,0,q),.6+i*.15,
        -.09,-.12+Math.sin(local*.4+i)*.05,(i-1)*.05,q);
    });
  } else if(index===6){
    show(network,0,-.15,0,-.06,Math.sin(local*.22)*.09,0,enter);
    nodes.forEach((node,i)=>{node.position.y=nodePositions[i].y+Math.sin(local*.8+i)*.08;node.rotation.y=local*.15;});
    trails.forEach(({curve,particle},i)=>particle.position.copy(curve.getPoint((local*.22+i*.4)%1)));
    networkLabels.forEach((card,i)=>show(card,[-3.5,3.5,0][i],[-1.7,-1.45,-2.65][i],.6,0,i===0?.13:-.1,0,out((local-.4-i*.13)/1)));
  } else if(index===7){
    prep.forEach((card,i)=>{const q=out((local-.12-i*.15)/1.2);
      show(card,(i-1)*3.65,mix(-3.3,-.65,q)+Math.sin(local*.7+i)*.055,.3+Math.cos(local*.5+i)*.08,
        mix(.3,-.04,q),mix((i-1)*.3,(i-1)*-.055,q),(i-1)*-.015,q);
    });
  } else if(index===8){
    $('backdrop').style.background='radial-gradient(ellipse at 50% 72%,#356e50,#122b23 67%,#0e201c)';
    $('copy').style.cssText=`top:304px;left:150px;right:auto;opacity:${out(local/.65)};text-align:left;transform:translateY(${mix(45,0,enter)}px);color:#edfff0`;
    $('headline').style.fontSize='104px';$('headline').style.lineHeight='1.5';
    $('eyebrow').style.color='#abcfb8';$('brand').style.opacity='0';
    show(bloom,3.8,-.65,-2.7,.55,.18,local*.09,1.25);
    bloom.children.slice(0,5).forEach(m=>m.scale.setScalar(m.userData.baseScale));
    camera.position.z=mix(12,10.9,ease(p));
  } else if(index===9){
    $('copy').style.opacity='0';$('caption').style.opacity='0';
    show(logo,-2.67,1.01,.5,mix(.15,-.035,enter),mix(-.3,.05,enter),-.035,.71);
    $('closing').style.opacity=String(out((local-.25)/1.1));
    $('closing').style.transform=`translateY(${mix(22,0,enter)}px)`;
    camera.position.z=11.7;
  }
  // Brief directional light wipe ties the edit together without a slide UI.
  const wipe=index>0?Math.max(0,1-local/.34)*.36:0;
  $('wash').style.opacity=String(wipe);
  $('wash').style.transform=`translateX(${mix(-900,900,clamp(local/.4))}px)`;
  $('fade').style.opacity=String(t<.6?1-out(t/.6):t>59.65?ease((t-59.65)/.35):0);
  renderer.render(scene,camera);
};
await document.fonts.ready;
window.renderAt(0);
window.ready=true;

// The same source can be reviewed interactively at a chosen timestamp.
const params=new URLSearchParams(location.search);
if(params.has('t'))window.renderAt(Number(params.get('t'))||0);
