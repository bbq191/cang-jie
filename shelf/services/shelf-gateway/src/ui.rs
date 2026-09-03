//! 单页 UI（手机/电脑浏览器打开 `http://<设备IP>:8778/`）。tab 按 `/api/services` 动态生成；
//! 传书 tab 的目标下拉按 target 打不同路由（native/annot → /api/books，koreader → /api/koreader/books）。
//! 上传逐文件一请求（每本独立成败、独立进度条）；字体/壁纸 tab 复用同一上传器（AssetUploadFlow 回执同形）。
pub const PAGE: &str = r##"<!doctype html><html lang="zh"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>书架</title>
<style>
:root{--fg:#111;--mute:#666;--line:#ddd;--ok:#1a7f37;--bad:#b3261e;--bg:#fff}
body{font-family:system-ui,-apple-system,"PingFang SC","Noto Sans CJK SC",sans-serif;margin:0;color:var(--fg);background:var(--bg)}
header{padding:.8em 1em;border-bottom:1px solid var(--line);display:flex;align-items:baseline;gap:1em}
header h1{font-size:1.2em;margin:0}header small{color:var(--mute)}
nav{display:flex;gap:.2em;padding:.4em 1em 0;border-bottom:1px solid var(--line);overflow-x:auto}
nav button{border:1px solid var(--line);border-bottom:none;background:#f6f6f6;padding:.5em 1em;border-radius:.5em .5em 0 0;cursor:pointer;font-size:1em}
nav button.on{background:var(--bg);font-weight:600}
main{padding:1em;max-width:44em;margin:0 auto}
section{display:none}section.on{display:block}
.row{display:flex;gap:.6em;align-items:center;flex-wrap:wrap;margin:.6em 0}
select,input[type=text]{font-size:1em;padding:.4em}
.drop{border:2px dashed var(--line);border-radius:.6em;padding:1.4em;text-align:center;color:var(--mute);margin:.8em 0}
.drop.hi{border-color:var(--fg);color:var(--fg)}
ul.q{list-style:none;padding:0;margin:0}ul.q li{border:1px solid var(--line);border-radius:.4em;padding:.5em .7em;margin:.4em 0}
.name{font-weight:600;word-break:break-all}.msg{color:var(--mute);font-size:.92em}.ok .msg{color:var(--ok)}.bad .msg{color:var(--bad)}
progress{width:100%;height:.5em}
.btn{font-size:1em;padding:.45em .9em;border:1px solid var(--fg);background:var(--bg);border-radius:.4em;cursor:pointer}
.btn.pri{background:var(--fg);color:var(--bg)}.btn:disabled{opacity:.5}
.kv{display:grid;grid-template-columns:auto 1fr;gap:.2em 1em;font-size:.95em}.kv b{color:var(--mute);font-weight:500}
.list li{display:flex;justify-content:space-between;gap:1em;padding:.35em 0;border-bottom:1px solid var(--line)}
.small{font-size:.85em;color:var(--mute)}
</style></head><body>
<header><h1>书架</h1><small id="hdr">连接中…</small></header>
<nav id="tabs"></nav>
<main id="main"><p class="small">加载服务列表…</p></main>
<script>
const $=(s,r=document)=>r.querySelector(s);
const SEG={"book-serve":"books","koreader-serve":"koreader","font-serve":"fonts","wallpaper-serve":"wallpapers","weread-serve":"weread"};
const fmtB=n=>n>1048576?(n/1048576).toFixed(1)+' MB':n>1024?(n/1024).toFixed(0)+' KB':n+' B';
async function j(url,opt){const r=await fetch(url,opt);let d;try{d=await r.json()}catch{d={ok:false,message:'HTTP '+r.status}}if(!r.ok&&d.ok!==false)d={ok:false,message:d.message||('HTTP '+r.status)};return d}

/* 通用上传器：逐文件一请求，进度条，回执逐项 */
function uploader(box,urlOf,queryOf){
  const list=$('ul.q',box), input=$('input[type=file]',box), drop=$('.drop',box), go=$('.go',box);
  let files=[];
  const render=()=>{list.innerHTML='';files.forEach(f=>{const li=document.createElement('li');li.dataset.k=f.k;li.innerHTML=`<div class="name">${f.file.name} <span class="small">${fmtB(f.file.size)}</span></div><progress value="0" max="100"></progress><div class="msg">${f.msg||'待传'}</div>`;li.className=f.st||'';list.appendChild(li)})};
  const add=fl=>{for(const f of fl)files.push({file:f,k:Math.random().toString(36).slice(2)});render()};
  input.onchange=()=>add(input.files);
  drop.ondragover=e=>{e.preventDefault();drop.classList.add('hi')};drop.ondragleave=()=>drop.classList.remove('hi');
  drop.ondrop=e=>{e.preventDefault();drop.classList.remove('hi');add(e.dataTransfer.files)};
  drop.onclick=()=>input.click();
  go.onclick=async()=>{go.disabled=true;
    for(const f of files){if(f.st)continue;const li=list.querySelector(`li[data-k="${f.k}"]`),pg=$('progress',li),msg=$('.msg',li);
      msg.textContent='上传中…';
      await new Promise(res=>{const x=new XMLHttpRequest();const q=queryOf();x.open('POST',urlOf()+(q?'?'+new URLSearchParams(q):''));
        x.upload.onprogress=e=>{if(e.lengthComputable)pg.value=e.loaded/e.total*100};
        x.onload=()=>{let d;try{d=JSON.parse(x.responseText)}catch{d={ok:false,message:'HTTP '+x.status}}
          const it=(d.items&&d.items[0])||d;f.st=it.ok?'ok':'bad';f.msg=it.message||(it.ok?'完成':'失败');li.className=f.st;msg.textContent=f.msg;pg.value=100;res()};
        x.onerror=()=>{f.st='bad';f.msg='网络错误';li.className='bad';msg.textContent=f.msg;res()};
        const fd=new FormData();fd.append('file',f.file);x.send(fd)})}
    go.disabled=false};
  return {clear(){files=[];render()}};
}

const TABS={
 'book-serve':{title:'传书',render(sec){sec.innerHTML=`
  <div class="row"><label>投到 <select id="tgt"><option value="native">原生阅读（xochitl；AZW3/MOBI/FB2/CBZ 自动转换，EPUB 自动优化）</option><option value="annot">原生批注（PDF 定稿；只收 PDF/CBZ）</option><option value="koreader">KOReader（原样投递，任意格式）</option></select></label></div>
  <div class="row"><label>文件夹 <input type="text" id="folder" placeholder="留空=默认"></label><label><input type="checkbox" id="opt" checked> 设备端优化 EPUB</label></div>
  <div class="drop">点击或拖入文件（可多选）</div><input type="file" multiple hidden>
  <ul class="q"></ul><div class="row"><button class="btn pri go">开始上传</button><button class="btn clr">清空</button></div>
  <div id="bstat" class="kv small" style="margin-top:1em"></div><h3 style="font-size:1em">未完成 / 失败</h3><ul class="list" id="inbox"></ul>`;
  const tgt=$('#tgt',sec);
  const up=uploader(sec,()=>tgt.value==='koreader'?'/api/koreader/books':'/api/books',()=>{const q={folder:$('#folder',sec).value.trim()};if(tgt.value!=='koreader'){q.target=tgt.value;q.optimize=$('#opt',sec).checked?'auto':'off'}return q});
  $('.clr',sec).onclick=()=>up.clear();
  const refresh=async()=>{const s=await j('/api/books/status');$('#bstat',sec).innerHTML=s.ok?`<b>xochitl /upload</b><span>${s.uploadReachable?'可达':'<span style="color:var(--bad)">不可达（lo 别名/USB 未就绪）</span>'}</span><b>书库文件夹</b><span>${s.libraryFolder} / 批注 ${s.annotFolder}</span><b>队列</b><span>待处理 ${s.spool.pending} · 失败 ${s.spool.failed}</span>`:`<b>book-serve</b><span>${s.message}</span>`;
    const ib=await j('/api/books/inbox');const ul=$('#inbox',sec);ul.innerHTML='';(ib.items||[]).forEach(it=>{const li=document.createElement('li');li.innerHTML=`<span>${it.name} <span class="small">${it.state} · ${fmtB(it.bytes)}</span></span><span>${it.state==='failed'?'<button class="btn r">重试</button> <button class="btn d">删除</button>':''}</span>`;
      if(it.state==='failed'){$('.r',li).onclick=async()=>{await j('/api/books/inbox/retry',{method:'POST',body:JSON.stringify({name:it.name})});refresh()};$('.d',li).onclick=async()=>{await j('/api/books/inbox/delete',{method:'POST',body:JSON.stringify({name:it.name})});refresh()}}
      ul.appendChild(li)});if(!(ib.items||[]).length)ul.innerHTML='<li class="small">（空）</li>'};
  refresh();sec.dataset.refresh='1';sec.refresh=refresh;}},
 'koreader-serve':{title:'KOReader',render(sec){sec.innerHTML=`<div class="kv" id="ks">加载…</div><h3 style="font-size:1em">books/</h3><ul class="list" id="kb"></ul>`;
  const refresh=async()=>{const s=await j('/api/koreader/status');$('#ks',sec).innerHTML=s.ok?`<b>安装</b><span>${s.installed?'是':'否'} ${s.version?'('+s.version+')':''}</span><b>运行中</b><span>${s.running?'是（改配置须先退出）':'否'}</span><b>目录</b><span>${s.root}</span><b>书</b><span>${s.books} 本 · 字体 ${s.fonts} 个</span>`:`<span>${s.message}</span>`;
    const b=await j('/api/koreader/books');const ul=$('#kb',sec);ul.innerHTML='';(b.items||[]).forEach(it=>{const li=document.createElement('li');li.innerHTML=`<span>${it.name}</span><span class="small">${fmtB(it.bytes)}</span>`;ul.appendChild(li)});if(!(b.items||[]).length)ul.innerHTML='<li class="small">（空）</li>'};
  refresh();sec.refresh=refresh}},
 'font-serve':{title:'字体',render(sec){assetTab(sec,'/api/fonts','ttf/otf 字体（同时装进原生阅读器与 KOReader）')}},
 'wallpaper-serve':{title:'壁纸',render(sec){assetTab(sec,'/api/wallpapers','jpg/png 图片（自动裁到 954×1696，首张自动启用，下次休眠即生效）',{
   header:`<div class="row"><label>轮换 <select id="wpmode"><option value="sequential">按顺序</option><option value="random">随机</option><option value="fixed">固定</option></select></label><span id="wpst" class="small"></span></div>`,
   onRender:async(sec,refresh)=>{const st=await j('/api/wallpapers/status');const sel=$('#wpmode',sec);if(st.ok){sel.value=st.mode;$('#wpst',sec).textContent=`当前 ${st.current||'（无）'} · bind ${st.mounted}/${st.expectedMounts}`}
     sel.onchange=async()=>{await j('/api/wallpapers/mode',{method:'PUT',body:JSON.stringify({mode:sel.value})});refresh()}},
   itemAction:(it,refresh)=>{if((it.extra||{}).current)return '<span class="small">当前</span>';const b=document.createElement('button');b.className='btn';b.textContent='使用';b.onclick=async()=>{await j('/api/wallpapers/current',{method:'PUT',body:JSON.stringify({name:it.name})});refresh()};return b},
   preview:it=>`<img src="/api/wallpapers/${encodeURIComponent(it.name)}" alt="" style="height:3.6em;border:1px solid var(--line);margin-right:.5em;vertical-align:middle">`})}},
 'weread-serve':{title:'微读',render(sec){sec.innerHTML='<p class="small">微信读书网页版入口（预留）。</p>'}}
};
function assetTab(sec,api,hint,ext={}){sec.innerHTML=`<p class="small">${hint}</p>${ext.header||''}<div class="drop">点击或拖入文件（可多选）</div><input type="file" multiple hidden><ul class="q"></ul><div class="row"><button class="btn pri go">上传</button></div><h3 style="font-size:1em">已安装</h3><ul class="list" id="al"></ul>`;
  uploader(sec,()=>api,()=>({}));
  const refresh=async()=>{const d=await j(api);const ul=$('#al',sec);ul.innerHTML='';(d.items||[]).forEach(it=>{const ex=it.extra||{};const li=document.createElement('li');
    const left=document.createElement('span');left.innerHTML=(ext.preview?ext.preview(it):'')+`${it.name}${ex.family?' <span class="small">'+ex.family+'</span>':''}`;
    const right=document.createElement('span');right.className='small';right.textContent=(it.bytes?fmtB(it.bytes)+' ':'');
    if(ext.itemAction){const a=ext.itemAction(it,refresh);if(typeof a==='string')right.insertAdjacentHTML('beforeend',a);else right.appendChild(a);right.append(' ')}
    if(ex.source==='builtin')right.append('内建');else if(!ex.current){const del=document.createElement('button');del.className='btn';del.textContent='删除';del.onclick=async()=>{if(confirm('删除 '+it.name+'？')){const r=await j(api+'/'+encodeURIComponent(it.name),{method:'DELETE'});if(r.ok===false)alert(r.message);refresh()}};right.appendChild(del)}
    li.append(left,right);ul.appendChild(li)});if(!ul.children.length)ul.innerHTML='<li class="small">（空）</li>';if(ext.onRender)ext.onRender(sec,refresh)};
  refresh();sec.refresh=refresh}

(async()=>{const d=await j('/api/services');const svcs=(d.services||[]).filter(s=>s.ui&&TABS[s.name]).sort((a,b)=>a.ui.order-b.ui.order);
  $('#hdr').textContent=svcs.length?location.host:'无领域服务在线';
  const nav=$('#tabs'),main=$('#main');main.innerHTML='';
  svcs.forEach((s,i)=>{const b=document.createElement('button');b.textContent=TABS[s.name].title;const sec=document.createElement('section');sec.id='t-'+s.name;
    b.onclick=()=>{[...nav.children].forEach(x=>x.classList.remove('on'));[...main.children].forEach(x=>x.classList.remove('on'));b.classList.add('on');sec.classList.add('on');if(sec.refresh)sec.refresh()};
    nav.appendChild(b);main.appendChild(sec);TABS[s.name].render(sec);if(i===0)b.onclick()});
  if(!svcs.length)main.innerHTML='<p>没有领域服务在线。用 <code>shelf/install.sh</code> 安装，或检查 <code>systemctl status shelf.target</code>。</p>';
})();
</script></body></html>"##;
