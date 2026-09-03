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
<header><h1>书架</h1><small id="hdr">连接中…</small><span style="margin-left:auto" class="small"><a href="/password">改密码</a> · <a href="/ca.crt">CA 证书</a> · <a href="#" id="logout">退出</a></span></header>
<nav id="tabs"></nav>
<main id="main"><p class="small">加载服务列表…</p></main>
<script>
const $=(s,r=document)=>r.querySelector(s);
const SEG={"book-serve":"books","koreader-serve":"koreader","font-serve":"fonts","wallpaper-serve":"wallpapers","weread-serve":"weread"};
const fmtB=n=>n>1048576?(n/1048576).toFixed(1)+' MB':n>1024?(n/1024).toFixed(0)+' KB':n+' B';
document.getElementById('logout').onclick=e=>{e.preventDefault();fetch('/logout',{method:'POST'}).then(()=>location.href='/login')};
async function j(url,opt){const r=await fetch(url,opt);if(r.status===401){location.href='/login?next='+encodeURIComponent(location.pathname);return {ok:false,message:'未登录'}}if(r.status===403){location.href='/password';return {ok:false,message:'需先改密码'}}let d;try{d=await r.json()}catch{d={ok:false,message:'HTTP '+r.status}}if(!r.ok&&d.ok!==false)d={ok:false,message:d.message||('HTTP '+r.status)};return d}

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
        x.onload=()=>{if(x.status===401){location.href='/login';return}let d;try{d=JSON.parse(x.responseText)}catch{d={ok:false,message:'HTTP '+x.status}}
          const it=(d.items&&d.items[0])||d;f.st=it.ok?'ok':'bad';f.msg=it.message||(it.ok?'完成':'失败');li.className=f.st;msg.textContent=f.msg;pg.value=100;res()};
        x.onerror=()=>{f.st='bad';f.msg='网络错误';li.className='bad';msg.textContent=f.msg;res()};
        const fd=new FormData();fd.append('file',f.file);x.send(fd)})}
    go.disabled=false};
  return {clear(){files=[];render()}};
}

const TABS={
 'book-serve':{title:'传书',render(sec){sec.innerHTML=`
  <div class="row"><label>投到 <select id="tgt"><option value="native">原生阅读（xochitl；AZW3/MOBI/FB2/CBZ 自动转换，EPUB 自动优化）</option><option value="annot">原生批注（PDF 定稿；只收 PDF/CBZ）</option><option value="koreader">KOReader（原样投递，任意格式）</option></select></label></div>
  <div class="row"><label>文件夹 <input type="text" id="folder" placeholder="留空=默认；KOReader 可多级 如 漫画/阿拉蕾"></label></div>
  <div class="row" id="optrow"><label>EPUB 处理 <select id="opt"><option value="auto">清洗 + 优化（对标电脑洗书：剥字体/颜色锁、边距段距归零、缺目录自动建）</option><option value="keep-spacing">清洗但保留段距（诗集 / 剧本）</option><option value="plain">只优化不清洗（脚注/图片/对比度）</option><option value="off">原样进库</option></select></label><label><input type="checkbox" id="chk" checked> 质量门</label> <span class="small">（真 DRM / 目录坏 / 双 id 硬拦；不勾=强行投递。Calibre 转换只在电脑 <code>shelf push</code>）</span></div>
  <div class="drop">点击或拖入文件（可多选）</div><input type="file" multiple hidden>
  <ul class="q"></ul><div class="row"><button class="btn pri go">开始上传</button><button class="btn clr">清空</button></div>
  <div id="bstat" class="kv small" style="margin-top:1em"></div><h3 style="font-size:1em">未完成 / 失败</h3><ul class="list" id="inbox"></ul>`;
  const tgt=$('#tgt',sec);
  const up=uploader(sec,()=>tgt.value==='koreader'?'/api/koreader/books':'/api/books',()=>{const q={folder:$('#folder',sec).value.trim()};if(tgt.value!=='koreader'){q.target=tgt.value;q.optimize=$('#opt',sec).value;q.check=$('#chk',sec).checked?'on':'off'}return q});
  $('.clr',sec).onclick=()=>up.clear();
  const refresh=async()=>{const s=await j('/api/books/status');$('#bstat',sec).innerHTML=s.ok?`<b>xochitl /upload</b><span>${s.uploadReachable?'可达':'<span style="color:var(--bad)">不可达（lo 别名/USB 未就绪）</span>'}</span><b>书库文件夹</b><span>${s.libraryFolder} / 批注 ${s.annotFolder}</span><b>队列</b><span>待处理 ${s.spool.pending} · 失败 ${s.spool.failed}</span>${s.readingQol?`<b>阅读增强</b><span class="small">点击翻页 ${s.readingQol.tapPageTurn?'开':'关'} · 快速黑白 ${s.readingQol.fastMono?'开':'关'} · 清残影 ${s.readingQol.refresh?'开':'关'} · 字体增强 ${s.readingQol.fontEnhance?'开':'关'}（设置→系统增强 里改）</span>`:''}`:`<b>book-serve</b><span>${s.message}</span>`;
    const ib=await j('/api/books/inbox');const ul=$('#inbox',sec);ul.innerHTML='';(ib.items||[]).forEach(it=>{const li=document.createElement('li');li.innerHTML=`<span>${it.name} <span class="small">${it.state} · ${fmtB(it.bytes)}</span></span><span>${it.state==='failed'?'<button class="btn r">重试</button> <button class="btn d">删除</button>':''}</span>`;
      if(it.state==='failed'){$('.r',li).onclick=async()=>{await j('/api/books/inbox/retry',{method:'POST',body:JSON.stringify({name:it.name})});refresh()};$('.d',li).onclick=async()=>{await j('/api/books/inbox/delete',{method:'POST',body:JSON.stringify({name:it.name})});refresh()}}
      ul.appendChild(li)});if(!(ib.items||[]).length)ul.innerHTML='<li class="small">（空）</li>'};
  refresh();sec.dataset.refresh='1';sec.refresh=refresh;}},
 'koreader-serve':{title:'KOReader',render(sec){sec.innerHTML=`<div class="kv" id="ks">加载…</div>
  <h3 style="font-size:1em">传字体给 KOReader <span class="small">（只装进 KOReader；原生阅读器的字体去「字体」页装）</span></h3>
  <div class="drop">点击或拖入 ttf/otf（可多选）</div><input type="file" multiple hidden accept=".ttf,.otf,.ttc"><ul class="q"></ul><div class="row"><button class="btn pri go">上传</button></div>
  <h3 style="font-size:1em">KOReader fonts/</h3><ul class="list" id="kf"></ul>
  <h3 style="font-size:1em">books/ <span id="kcrumb" class="small"></span></h3>
  <div class="row"><div class="drop" style="flex:1;margin:0;padding:.6em">拖入书到当前目录（可多选）</div><input type="file" multiple hidden></div><ul class="q"></ul><div class="row"><button class="btn pri go">上传到当前目录</button></div>
  <ul class="list" id="kb"></ul>`;
  let kdir='';
  // 两个上传器：字体（第一组 drop/input/q/go）与书（第二组）——按 DOM 顺序取
  const fontBox=document.createElement('div'),bookBox=document.createElement('div');
  const drops=sec.querySelectorAll('.drop'),inputs=sec.querySelectorAll('input[type=file]'),qs=sec.querySelectorAll('ul.q'),gos=sec.querySelectorAll('.go');
  const wrap=(i)=>({querySelector:(sel)=>({'ul.q':qs[i],'input[type=file]':inputs[i],'.drop':drops[i],'.go':gos[i]}[sel])});
  const $$=$;
  uploader(wrap(0),()=>'/api/koreader/fonts',()=>({}));
  uploader(wrap(1),()=>'/api/koreader/books',()=>({folder:kdir}));
  const refresh=async()=>{const s=await j('/api/koreader/status');$('#ks',sec).innerHTML=s.ok?`<b>安装</b><span>${s.installed?'是':'否'} ${s.version?'('+s.version+')':''}</span><b>运行中</b><span>${s.running?'是（改配置/删字体后需重启它）':'否'}</span><b>目录</b><span>${s.root}</span><b>书</b><span>${s.books} 本 · 字体 ${s.fonts} 个</span>`:`<span>${s.message}</span>`;
    const f=await j('/api/koreader/fonts');const uf=$('#kf',sec);uf.innerHTML='';(f.items||[]).forEach(it=>{const li=document.createElement('li');li.innerHTML=`<span>${it.name}</span><span class="small">${fmtB(it.bytes)} </span>`;const d=document.createElement('button');d.className='btn';d.textContent='删除';d.onclick=async()=>{if(confirm('从 KOReader 删除 '+it.name+'？')){const r=await j('/api/koreader/fonts/'+encodeURIComponent(it.name),{method:'DELETE'});if(r.ok===false)alert(r.message);refresh()}};li.lastChild.appendChild(d);uf.appendChild(li)});if(!(f.items||[]).length)uf.innerHTML='<li class="small">（空）</li>';
    const b=await j('/api/koreader/books?'+new URLSearchParams({folder:kdir}));const ul=$('#kb',sec);ul.innerHTML='';
    const crumb=$('#kcrumb',sec);crumb.innerHTML='';const parts=kdir?kdir.split('/'):[];const mk=(t,p)=>{const a=document.createElement('a');a.href='#';a.textContent=t;a.onclick=e=>{e.preventDefault();kdir=p;refresh()};return a};crumb.appendChild(mk('根',''));parts.forEach((p,i)=>{crumb.append(' / ');crumb.appendChild(mk(p,parts.slice(0,i+1).join('/')))});
    (b.items||[]).forEach(it=>{const li=document.createElement('li');if(it.kind==='dir'){li.innerHTML=`<span>📁 <a href="#">${it.name}</a></span><span class="small">${it.count} 本</span>`;$('a',li).onclick=e=>{e.preventDefault();kdir=(kdir?kdir+'/':'')+it.name;refresh()}}else li.innerHTML=`<span>${it.name}</span><span class="small">${fmtB(it.bytes)}</span>`;ul.appendChild(li)});if(!(b.items||[]).length)ul.innerHTML='<li class="small">（空目录）</li>'};
  refresh();sec.refresh=refresh}},
 'font-serve':{title:'字体',render(sec){assetTab(sec,'/api/fonts','ttf/otf 字体 → 只装进原生阅读器（fontconfig 用户字体目录）；KOReader 的字体去 KOReader 页装。列表=目录里全部字体，按家族归组，都可删')}},
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
    const left=document.createElement('span');left.innerHTML=(ext.preview?ext.preview(it):'')+`${it.name}${ex.names&&ex.names.cn&&ex.names.cn!==it.name?' <span class="small">'+ex.names.cn+'</span>':''}${ex.files&&ex.files.length>1?' <span class="small">×'+ex.files.length+'</span>':''}`;
    const right=document.createElement('span');right.className='small';right.textContent=(it.bytes?fmtB(it.bytes)+' ':'');
    if(ext.itemAction){const a=ext.itemAction(it,refresh);if(typeof a==='string')right.insertAdjacentHTML('beforeend',a);else right.appendChild(a);right.append(' ')}
    if(ex.fontconfigRef)right.insertAdjacentHTML('afterbegin','<span title="被 fontconfig 配置引用（界面中文回退用），删了界面可能变方块">⚠ </span>');
    if(!ex.current){const del=document.createElement('button');del.className='btn';del.textContent='删除';del.onclick=async()=>{const files=(ex.files||[]).length>1?'（含 '+ex.files.length+' 个文件）':'';if(confirm('删除 '+it.name+files+(ex.fontconfigRef?'？\n⚠ 该字体是界面中文回退字体':'？'))){const r=await j(api+'/'+encodeURIComponent(it.name),{method:'DELETE'});if(r.ok===false)alert(r.message);refresh()}};right.appendChild(del)}
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


const AUTH_CSS: &str = r#"body{font-family:system-ui,-apple-system,"PingFang SC","Noto Sans CJK SC",sans-serif;margin:0;background:#fff;color:#111;display:flex;min-height:100vh;align-items:center;justify-content:center}
form{width:min(22em,90vw);border:1px solid #ddd;border-radius:.8em;padding:1.4em 1.6em}
h1{font-size:1.2em;margin:0 0 .6em}label{display:block;margin:.7em 0 .2em;color:#666;font-size:.92em}
input{width:100%;box-sizing:border-box;font-size:1.05em;padding:.5em}
button{margin-top:1em;width:100%;font-size:1em;padding:.55em;border:1px solid #111;background:#111;color:#fff;border-radius:.4em;cursor:pointer}
.err{color:#b3261e;margin:.6em 0 0;min-height:1.2em}.small{font-size:.85em;color:#666;margin-top:1em;line-height:1.5}a{color:inherit}"#;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// 登录页：只要密码，无用户名。`error` 空=无提示。
pub fn login_page(error: &str, next: &str) -> String {
    format!(r#"<!doctype html><html lang="zh"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>书架 · 登录</title><style>{AUTH_CSS}</style></head><body>
<form method="post" action="/login" autocomplete="on"><h1>书架</h1>
<label for="pw">密码</label><input id="pw" name="password" type="password" autofocus required autocomplete="current-password">
<input type="hidden" name="next" value="{next}"><div class="err">{err}</div><button type="submit">登录</button>
<p class="small">首次使用密码为 <code>shelf</code>，登录后必须改。<br>浏览器提示"不安全"是自签证书所致：<a href="/ca.crt">下载 CA 证书</a> 装进手机/电脑信任库一次即不再提示。</p></form></body></html>"#, next = esc(next), err = esc(error))
}

/// 改密码页：`forced`=首登必改（不给"返回"）。
pub fn password_page(error: &str, forced: bool) -> String {
    let hint = if forced { "首次登录：请先设置新密码（至少 6 位，不能是默认密码）。" } else { "至少 6 位。改完其它已登录设备需重新登录。" };
    let back = if forced { "" } else { r#"<p class="small"><a href="/">返回书架</a></p>"# };
    format!(r#"<!doctype html><html lang="zh"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>书架 · 改密码</title><style>{AUTH_CSS}</style></head><body>
<form method="post" action="/password"><h1>设置密码</h1><p class="small" style="margin-top:0">{hint}</p>
<label for="cur">当前密码</label><input id="cur" name="current" type="password" required autocomplete="current-password">
<label for="new">新密码</label><input id="new" name="new" type="password" required minlength="6" autocomplete="new-password">
<label for="cf">再输一次</label><input id="cf" name="confirm" type="password" required minlength="6" autocomplete="new-password">
<div class="err">{err}</div><button type="submit">保存</button>{back}</form></body></html>"#, err = esc(error))
}
