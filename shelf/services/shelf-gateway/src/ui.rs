//! 单页 UI（手机/电脑浏览器打开 `https://<设备IP>:8778/`）。固定 tab「传书」（母版库总入口）+「管理」，中间的服务 tab
//! 按 `/api/services` 注册表动态生成（xochitl 字体 = font-serve、KOReader = koreader-serve、壁纸 = wallpaper-serve）。
//! 上传逐文件一请求（每本独立成败、独立进度条），所有上传口共用一个 `uploader` + 服务端同形回执（`asset::receipt`）；
//! 格式白名单由 [`page`] 从 `shelf_core::formats` 注入（`__EXTS__`），网页 accept / 选中即拦与服务端上传门同源。
use std::sync::OnceLock;

/// 渲染主页：把格式白名单注入模板（进程内只算一次）。
pub fn page() -> &'static str {
    static PAGE: OnceLock<String> = OnceLock::new();
    PAGE.get_or_init(|| {
        use shelf_core::formats::{BOOK_EXTS, DICT_EXTS, FONT_EXTS, IMAGE_EXTS};
        let exts = serde_json::json!({"book": BOOK_EXTS, "font": FONT_EXTS, "dict": DICT_EXTS, "image": IMAGE_EXTS});
        PAGE_TEMPLATE.replace("__EXTS__", &exts.to_string())
    })
}

const PAGE_TEMPLATE: &str = r##"
<!doctype html><html lang="zh"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>书架</title>
<style>
:root{color-scheme:light dark;
 --bg:#f6f7f9;--surface:#fff;--surface2:#f0f2f5;--fg:#1a1c1e;--mute:#5b6169;--line:#dfe3e8;--line2:#eceef1;
 --accent:#3257d0;--accent-fg:#fff;--ok:#1a7f37;--bad:#c0362c;--warn:#9a6700;--shadow:0 1px 2px rgba(0,0,0,.05),0 4px 14px rgba(0,0,0,.05)}
@media (prefers-color-scheme:dark){:root{
 --bg:#16181c;--surface:#1e2126;--surface2:#262a30;--fg:#e6e8eb;--mute:#9aa2ad;--line:#333941;--line2:#2a2f36;
 --accent:#7d9bff;--accent-fg:#10131a;--ok:#5fce7f;--bad:#ff8c80;--warn:#e3b341;--shadow:0 1px 2px rgba(0,0,0,.4),0 6px 18px rgba(0,0,0,.35)}}
*{box-sizing:border-box;-webkit-tap-highlight-color:transparent}
body{font-family:system-ui,-apple-system,"PingFang SC","Noto Sans CJK SC",sans-serif;margin:0;color:var(--fg);background:var(--bg);line-height:1.5;-webkit-text-size-adjust:100%;text-size-adjust:100%}
a{color:var(--accent)}
/* 头部：纯色兜底（老 Safari 不认 color-mix 时头部仍不透明、正文滚过不透字）+ -webkit- 前缀让 iOS Safari 也模糊 */
header{position:sticky;top:0;z-index:5;background:var(--surface);background:color-mix(in srgb,var(--surface) 88%,transparent);-webkit-backdrop-filter:blur(8px);backdrop-filter:blur(8px);border-bottom:1px solid var(--line);padding:.7em 1.1em;display:flex;align-items:center;gap:.8em}
header .logo{font-size:1.15em;font-weight:700;letter-spacing:.02em;display:flex;align-items:center;gap:.4em}
header .logo::before{content:"📚";font-size:1.1em}
header small{color:var(--mute)}
header .links{margin-left:auto;display:flex;gap:.9em;font-size:.85em}
header .links a{color:var(--mute);text-decoration:none}header .links a:hover{color:var(--accent)}
nav{display:flex;gap:.4em;padding:.7em 1.1em;overflow-x:auto;background:var(--bg);position:sticky;top:3.1em;z-index:4;-webkit-overflow-scrolling:touch}
nav button{border:1px solid transparent;background:var(--surface2);color:var(--mute);padding:.45em 1.05em;border-radius:2em;cursor:pointer;font-size:.95em;white-space:nowrap;transition:.15s}
nav button:hover{color:var(--fg)}
nav button.on{background:var(--accent);color:var(--accent-fg);font-weight:600;box-shadow:var(--shadow)}
main{padding:1.1em;max-width:48em;margin:0 auto}
section{display:none;animation:fade .2s ease}section.on{display:block}
@keyframes fade{from{opacity:0;transform:translateY(4px)}to{opacity:1;transform:none}}
.card{background:var(--surface);border:1px solid var(--line);border-radius:.9em;padding:1em 1.1em;box-shadow:var(--shadow);margin:.9em 0}
h2{font-size:1.05em;margin:0 0 .1em}h3{font-size:.98em;margin:1.2em 0 .3em}
.lead{color:var(--mute);font-size:.9em;margin:.1em 0 0}
label.field{display:block;margin:.9em 0 .2em;font-size:.85em;color:var(--mute);font-weight:500}
select,input[type=text]{font-size:1em;padding:.55em .6em;border:1px solid var(--line);border-radius:.5em;background:var(--surface);color:var(--fg);width:100%;max-width:100%}
.row{display:flex;gap:.7em;align-items:center;flex-wrap:wrap;margin:.6em 0}
/* 目标选择卡 */
.seg{display:grid;grid-template-columns:repeat(3,1fr);gap:.6em;margin:.4em 0 .2em}
@media(max-width:34em){.seg{grid-template-columns:1fr}}
.seg label{position:relative;display:block;border:1.5px solid var(--line);border-radius:.7em;padding:.7em .8em;cursor:pointer;background:var(--surface);transition:.15s}
.seg label:hover{border-color:var(--accent)}
.seg input{position:absolute;opacity:0;pointer-events:none}
.seg label:has(input:checked){border-color:var(--accent);background:color-mix(in srgb,var(--accent) 10%,var(--surface));box-shadow:var(--shadow)}
.seg .t{font-weight:700;font-size:.98em}.seg .d{font-size:.82em;color:var(--mute);margin-top:.15em}
.seg .fmt{font-size:.72em;color:var(--accent);margin-top:.35em;font-variant:tabular-nums}
.opt-note{font-size:.82em;color:var(--mute);margin:.4em 0 0;padding:.5em .7em;background:var(--surface2);border-radius:.5em;border-left:3px solid var(--accent)}
.toggle{display:inline-flex;align-items:center;gap:.45em;font-size:.92em;cursor:pointer}
.drop{border:2px dashed var(--line);border-radius:.8em;padding:1.6em 1em;text-align:center;color:var(--mute);margin:.9em 0;transition:.15s;cursor:pointer}
.drop:hover,.drop.hi{border-color:var(--accent);color:var(--fg);background:color-mix(in srgb,var(--accent) 6%,transparent)}
.drop .big{font-size:1.6em;display:block;margin-bottom:.2em;opacity:.7}
ul.q{list-style:none;padding:0;margin:.6em 0}
ul.q li{border:1px solid var(--line);border-radius:.6em;padding:.55em .8em;margin:.45em 0;background:var(--surface)}
ul.q li.ok{border-color:color-mix(in srgb,var(--ok) 45%,var(--line))}ul.q li.bad{border-color:color-mix(in srgb,var(--bad) 45%,var(--line))}
.name{font-weight:600;word-break:break-all}.msg{color:var(--mute);font-size:.88em;margin-top:.2em}.ok .msg{color:var(--ok)}.bad .msg{color:var(--bad)}
progress{width:100%;height:.4em;margin:.35em 0;border:none;border-radius:1em;overflow:hidden}
progress::-webkit-progress-bar{background:var(--surface2)}progress::-webkit-progress-value{background:var(--accent)}
.btn{font-size:.95em;padding:.5em 1em;border:1px solid var(--line);background:var(--surface);color:var(--fg);border-radius:.5em;cursor:pointer;transition:.15s}
.btn:hover{border-color:var(--accent)}
.btn.pri{background:var(--accent);color:var(--accent-fg);border-color:var(--accent);font-weight:600}
.btn.pri:hover{filter:brightness(1.06)}.btn:disabled{opacity:.5;cursor:default}
.kv{display:grid;grid-template-columns:auto 1fr;gap:.35em .9em;font-size:.92em;align-items:baseline}.kv b{color:var(--mute);font-weight:500}
.list{list-style:none;padding:0;margin:.3em 0}
.list li{display:flex;justify-content:space-between;gap:1em;align-items:center;padding:.5em .1em;border-bottom:1px solid var(--line2)}
.small{font-size:.83em;color:var(--mute)}
code{background:var(--surface2);padding:.1em .35em;border-radius:.3em;font-size:.88em}
/* 对比表 */
details.cmp{margin:.6em 0}
details.cmp>summary{cursor:pointer;font-size:.9em;color:var(--accent);padding:.4em 0;user-select:none;list-style:none}
details.cmp>summary::-webkit-details-marker{display:none}
details.cmp>summary::before{content:"▸ ";transition:.15s;display:inline-block}
details.cmp[open]>summary::before{transform:rotate(90deg)}
.tblwrap{overflow-x:auto;margin:.4em 0;border:1px solid var(--line);border-radius:.6em}
table.cmp{border-collapse:collapse;width:100%;font-size:.85em;min-width:34em}
table.cmp th,table.cmp td{text-align:left;padding:.55em .7em;border-bottom:1px solid var(--line2);vertical-align:top}
table.cmp thead th{background:var(--surface2);font-weight:600;position:sticky;top:0}
table.cmp tbody th{font-weight:600;color:var(--fg);white-space:nowrap;background:var(--surface)}
table.cmp tbody tr:last-child td{border-bottom:none}
table.cmp .pick{color:var(--accent);font-weight:600}
.badge{display:inline-block;font-size:.72em;padding:.05em .5em;border-radius:1em;background:var(--surface2);color:var(--mute);border:1px solid var(--line)}
.badge.on{background:color-mix(in srgb,var(--ok) 18%,transparent);color:var(--ok);border-color:transparent}
.badge.off{background:color-mix(in srgb,var(--bad) 15%,transparent);color:var(--bad);border-color:transparent}
.crumb a{color:var(--accent);text-decoration:none}.crumb a:hover{text-decoration:underline}
/* 二级标签（把一个大页面拆成几屏，手机不长拉） */
.subnav{display:flex;gap:.4em;overflow-x:auto;margin:.2em 0 1em;padding-bottom:.15em;-webkit-overflow-scrolling:touch}
.subnav button{border:1px solid var(--line);background:var(--surface);color:var(--mute);padding:.32em .95em;border-radius:2em;cursor:pointer;font-size:.9em;white-space:nowrap;transition:.15s}
.subnav button:hover{color:var(--fg)}
.subnav button.on{background:var(--surface2);color:var(--fg);border-color:var(--accent);font-weight:600}
.subpanel{display:none}.subpanel.on{display:block}
/* 说明列表 */
dl.help{margin:.3em 0 .2em}dl.help dt{font-weight:600;margin-top:.7em;color:var(--fg)}
dl.help dd{margin:.2em 0 .2em;color:var(--mute);font-size:.9em;line-height:1.65}
</style></head><body>
<header><span class="logo">书架</span><small id="hdr">连接中…</small>
<span class="links"><a href="/password">改密码</a><a href="/ca.crt">CA 证书</a><a href="#" id="logout">退出</a></span></header>
<nav id="tabs"></nav>
<main id="main"><p class="small">加载服务列表…</p></main>
<script>
const $=(s,r=document)=>r.querySelector(s);
const fmtB=n=>n>1048576?(n/1048576).toFixed(1)+' MB':n>1024?(n/1024).toFixed(0)+' KB':n+' B';
/* 轻量记忆：per-viewer 便利态，隐私窗口/禁用 storage 时静默回默认 */
const LS={get(k,d){try{const v=localStorage.getItem('shelf.'+k);return v==null?d:v}catch{return d}},set(k,v){try{localStorage.setItem('shelf.'+k,v)}catch{}}};
const onUsb=/^10\.11\.99\./.test(location.hostname);
/* 格式白名单：服务端 shelf_core::formats 注入（同一份，网页 accept + 选中即拦 = 服务端上传门） */
const EXT=__EXTS__, dot=l=>l.map(e=>'.'+e);
const BOOK_EXT=dot(EXT.book), FONT_EXT=dot(EXT.font), DICT_EXT=dot(EXT.dict), IMG_EXT=dot(EXT.image);
$('#logout').onclick=e=>{e.preventDefault();fetch('/logout',{method:'POST'}).then(()=>location.href='/login')};
async function j(url,opt){const r=await fetch(url,opt);if(r.status===401){location.href='/login?next='+encodeURIComponent(location.pathname);return {ok:false,message:'未登录'}}if(r.status===403){location.href='/password';return {ok:false,message:'需先改密码'}}let d;try{d=await r.json()}catch{d={ok:false,message:'HTTP '+r.status}}if(!r.ok&&d.ok!==false)d={ok:false,message:d.message||('HTTP '+r.status)};return d}
const postJ=async(url,body)=>{const r=await j(url,{method:'POST',body:JSON.stringify(body)});if(r.ok===false)alert(r.message||'失败');return r};

/* 上传区 HTML（拖放框 + 隐藏 input + 队列 + 按钮），一处生成、各页复用；uploader() 认这个 .up 容器 */
const upHtml=(icon,label,ext,btn)=>`<div class="up"><div class="drop"><span class="big">${icon}</span>${label}</div><input type="file" multiple hidden accept="${ext.join(',')}"><ul class="q"></ul><div class="row"><button class="btn pri go">${btn}</button></div></div>`;
const extLabel=ext=>ext.map(e=>e.slice(1).toUpperCase()).join(' / ');

/* 通用上传器：逐文件一请求，进度条，逐项回执；失败项可重传，队列可逐项删/清空，顶部总进度。box=.up 容器 */
function uploader(box,urlOf,queryOf,okExt,onFinish){
  const list=$('ul.q',box), input=$('input[type=file]',box), drop=$('.drop',box), go=$('.go',box);
  let files=[], sum=null;
  const clr=document.createElement('button');clr.type='button';clr.className='btn';clr.textContent='清空';clr.onclick=()=>{files=[];render()};go.after(clr);
  const summary=()=>{if(!sum){sum=document.createElement('div');sum.className='small';sum.style.margin='.3em 0';list.parentNode.insertBefore(sum,list)}
    const ok=files.filter(f=>f.st==='ok').length,bad=files.filter(f=>f.st==='bad').length;
    sum.innerHTML=files.length?`${ok}/${files.length} 完成${bad?` · <span style="color:var(--bad)">${bad} 失败</span>`:''}`:'';};
  const render=()=>{list.innerHTML='';files.forEach(f=>{const li=document.createElement('li');li.dataset.k=f.k;li.className=f.st||'';
      li.innerHTML=`<div class="name">${f.file.name} <span class="small">${fmtB(f.file.size)}</span> <button class="btn x" type="button" title="移除" style="padding:.05em .45em;line-height:1">×</button></div><progress value="${f.st==='ok'?100:0}" max="100"></progress><div class="msg">${f.msg||'待传'}</div>`;
      li.querySelector('.x').onclick=()=>{files=files.filter(x=>x.k!==f.k);render()};list.appendChild(li)});summary()};
  const add=fl=>{for(const f of fl){const rej=okExt&&!okExt.some(e=>f.name.toLowerCase().endsWith(e));
      files.push({file:f,k:Math.random().toString(36).slice(2),rej,st:rej?'bad':'',msg:rej?('格式不收：只接受 '+okExt.join(' / ')):''})}render()};
  input.onchange=()=>{add(input.files);input.value=''};
  drop.ondragover=e=>{e.preventDefault();drop.classList.add('hi')};drop.ondragleave=()=>drop.classList.remove('hi');
  drop.ondrop=e=>{e.preventDefault();drop.classList.remove('hi');add(e.dataTransfer.files)};
  drop.onclick=()=>input.click();
  go.onclick=async()=>{go.disabled=true;clr.disabled=true;
    for(const f of files){if(f.st==='ok'||f.rej)continue;          // 成功项跳过；格式不收项不上传；失败项允许重传
      const li=list.querySelector(`li[data-k="${f.k}"]`);if(!li)continue;const pg=$('progress',li),msg=$('.msg',li);
      f.st='';li.className='';pg.value=0;msg.textContent='上传中…';
      await new Promise(res=>{const x=new XMLHttpRequest();const q=queryOf();x.open('POST',urlOf()+(q?'?'+new URLSearchParams(q):''));
        x.upload.onprogress=e=>{if(e.lengthComputable)pg.value=e.loaded/e.total*100};
        x.onload=()=>{if(x.status===401){location.href='/login';return}let d;try{d=JSON.parse(x.responseText)}catch{d={ok:false,message:'HTTP '+x.status}}
          const it=(d.items&&d.items[0])||d;f.st=it.ok?'ok':'bad';f.msg=(it.message||(it.ok?'完成':'失败'))+(d.note&&it.ok?' · '+d.note:'');li.className=f.st;msg.textContent=f.msg;pg.value=100;summary();res()};
        x.onerror=()=>{f.st='bad';f.msg='网络错误';li.className='bad';msg.textContent=f.msg;summary();res()};
        const fd=new FormData();fd.append('file',f.file);x.send(fd)})}
    go.disabled=false;clr.disabled=false;if(onFinish)onFinish()};
  return {clear(){files=[];render()}};
}

/* 二级标签：面板都由外层 render/refresh 预先填好，切换只显隐 */
function subtabs(sec){const nav=$('.subnav',sec);if(!nav)return;const btns=[...nav.children],panels=[...sec.querySelectorAll('.subpanel')];
  btns.forEach((b,i)=>b.onclick=()=>{btns.forEach(x=>x.classList.remove('on'));panels.forEach(p=>p.classList.remove('on'));b.classList.add('on');if(panels[i])panels[i].classList.add('on')});}

/* 列表渲染骨架：每项一行「左：名字等 ｜ 右：徽章/大小/按钮」；row(it,left,right,li) 填内容。字体/词典/壁纸共用 */
function fillList(ul,items,row){ul.innerHTML='';if(!items.length){ul.innerHTML='<li class="small">（空）</li>';return}
  items.forEach(it=>{const li=document.createElement('li');const left=document.createElement('span'),right=document.createElement('span');
    right.className='small';right.style.cssText='display:flex;align-items:center;gap:.4em;flex-wrap:wrap';row(it,left,right,li);li.append(left,right);ul.appendChild(li)})}
/* 删除按钮：confirm → DELETE → 刷新 */
function delBtn(msg,url,refresh){const d=document.createElement('button');d.className='btn';d.textContent='删除';
  d.onclick=async()=>{if(confirm(msg)){const r=await j(url,{method:'DELETE'});if(r.ok===false)alert(r.message);refresh()}};return d}
const cjkBadge=p=>p==null?'':`<span class="badge ${p>=80?'on':(p>=8?'':'off')}" title="中文基本区覆盖率">中文 ${p}%</span>`;

/* 决策辅助：不替用户分类（闲书/研读机器判不准），讲清母版库三步走 + 两读器各擅长；拿不准先投一个，母版还在 */
const GUIDE=`<details class="cmp"><summary>母版库怎么用？两个读器怎么选？（点开）</summary>
<dl class="help">
<dt>三步走</dt><dd>① <b>入库</b>：上传（书籍格式）/ 抓网文 / 微信读书 / 电脑 <code>shelf push</code>——书<b>原样</b>进母版库，不动字节。② <b>优化</b>（可选）：EPUB 点「优化」洗排版、脚注、中英文缩进（PDF 端上不动，重排走电脑）。③ <b>落库</b>：点「投入原生书库」或「加入 KOReader」。<b>母版留着</b>，随时再投另一个。读器页（xochitl / KOReader）只管各自的字体、词典，不传书。</dd>
<dt>格式</dt><dd>EPUB / PDF 两个读器都能去；CBZ / TXT / AZW3 等<b>只能加入 KOReader</b>，想进原生用电脑 <code>shelf push</code> 转成 EPUB。</dd>
<dt>📖 投入原生书库（xochitl）：要做笔记、批注的书</dt><dd>目录跳转、脚注、换字体、<b>直接手写批注</b>、AI 解读。学术 / 论文 / 要划线的书放这；PDF 手写定稿也放这。</dd>
<dt>📚 加入 KOReader：消遣、查词的书</dt><dd>自由重排、<b>内置词典</b>、翻页手势。小说、漫画、外语书顺手。</dd>
<dt>拿不准放哪？</dt><dd>先投一个。母版还在，觉得不对随时再投另一个对照——<b>不用纠结"闲书还是研读"，去向你说了算</b>（侦探小说有人当消遣、有人拿来推理画线索图；漫画有人看有人学画）。</dd>
<dt>电脑 shelf push（进阶）</dt><dd>难搞的书走电脑：非标准格式转 EPUB、Calibre 深洗、PDF 论文重排。洗完<b>也落这个母版库</b>，去向一样在这里选。命令见「管理」页。</dd>
</dl></details>`;

/* 母版库「优化」档位说明（对应 /staging/optimize 的 mode） */
const OPTTABLE=`<div class="tblwrap"><table class="cmp"><thead><tr><th>档位</th><th>做什么</th><th>什么时候用</th></tr></thead><tbody>
<tr><th class="pick">清洗＋优化<br><span class="small">默认</span></th><td>全套：剥字体/字号/颜色/对齐锁 · 边距段距归零+按中英文习惯首行缩进 · 缺目录按标题自动建 · 伪 DRM 剥离 · 脚注就地内联常显 · 远程图内联 · 图片降采样 · e-ink 提对比 · 双 id 去重</td><td>绝大多数第三方书</td></tr>
<tr><th>清洗但保留段距</th><td>同上，但不动原书段间距</td><td>诗集 / 剧本 / 靠空行分节的书</td></tr>
<tr><th>只优化不清洗</th><td>只修脚注 / 图片 / 对比度 / 双 id，不碰排版和字体锁</td><td>已排好版、只想修脚注和图的书</td></tr>
</tbody></table></div>`;

/* 母版库列表。按格式门控按钮：EPUB→优化(未优化时)/投原生/加入 KO；PDF→投原生/加入 KO；其它→只能加入 KO。
   「加入 KOReader」按 koInstalled 门控。opts: {items,q,fmt,st,xFolder(),kFolder(),mode(),clear(),koInstalled,refresh()} */
function stagingList(ul,opts){
  ul.innerHTML='';
  const q=(opts.q||'').toLowerCase();
  const items=(opts.items||[]).filter(it=>(!q||it.name.toLowerCase().includes(q))&&(!opts.fmt||it.format===opts.fmt)&&(!opts.st||(opts.st==='1')===!!it.optimized));
  if(!items.length){ul.innerHTML='<li class="small">'+(opts.items&&opts.items.length?'（没有匹配的书）':'（母版库是空的——去「入库」把书弄进来）')+'</li>';return}
  items.forEach(it=>{const li=document.createElement('li');li.style.flexWrap='wrap';
    const fmt=it.format==='epub'?'EPUB':it.format==='pdf'?'PDF':(it.name.includes('.')?it.name.split('.').pop().toUpperCase():'其它');
    const st=it.format!=='epub'?'<span class="badge">原样</span>':it.level==='full'?'<span class="badge on">已优化</span>':it.level==='core'?'<span class="badge" title="只跑了核心遍（脚注/图片/对比度），没洗排版缩进——点「优化」补全">已优化·未清洗</span>':it.level==='old'?'<span class="badge" title="旧版本优化，点「优化」升级">旧版优化</span>':'<span class="badge">未优化</span>';
    const hint=it.format==='pdf'?' · 手写定稿放原生':it.format==='other'?' · 原生读不了，只能加入 KOReader（想进原生用电脑 shelf push 转 EPUB）':'';
    // 落库记录徽章；落库时间早于母版 mtime（之后又优化过）→ 标「旧」，提示可重投
    const dv=it.delivered||{},stale=t=>t&&it.mtime&&t<it.mtime;
    const dl=(dv.native?` <span class="badge on" title="${stale(dv.native)?'投过，之后母版又优化过，可重投':'已投入原生书库'}">已投原生${stale(dv.native)?'·旧':''}</span>`:'')+(dv.koreader?` <span class="badge on" title="${stale(dv.koreader)?'加入过，之后母版又优化过，可重投':'已加入 KOReader'}">已加入KO${stale(dv.koreader)?'·旧':''}</span>`:'');
    li.innerHTML=`<span><b>${it.name}</b> <span class="badge">${fmt}</span> ${st}${dl} <span class="small">${fmtB(it.bytes)}${hint}</span></span>`;
    const right=document.createElement('span');right.style.cssText='display:flex;gap:.4em;flex-wrap:wrap;align-items:center';
    const btn=(t,pri,fn,dis,title)=>{const b=document.createElement('button');b.className='btn'+(pri?' pri':'');b.textContent=t;if(dis){b.disabled=true;b.title=title||''}else b.onclick=async()=>{b.disabled=true;b.textContent=t+'…';await fn();if(opts.refresh)opts.refresh()};right.appendChild(b)};
    if(it.format==='epub'&&!it.optimized)btn('优化',false,()=>postJ('/api/books/staging/optimize',{name:it.name,mode:opts.mode()}));
    if(it.format!=='other')btn('投入原生书库',true,()=>postJ('/api/books/staging/deliver',{name:it.name,keep:!opts.clear(),folder:opts.xFolder()}));
    btn('加入 KOReader',true,async()=>{const r=await postJ('/api/koreader/books/adopt',{name:it.name,folder:opts.kFolder()});if(r.ok!==false){await postJ('/api/books/staging/mark',{name:it.name,target:'koreader'});if(opts.clear())await postJ('/api/books/staging/delete',{name:it.name})}},!opts.koInstalled,'KOReader 未安装（「管理」页看基石）');
    btn('删除',false,async()=>{if(confirm('从母版库删除 '+it.name+'？（已投到读器的不受影响）'))await postJ('/api/books/staging/delete',{name:it.name})});
    li.appendChild(right);ul.appendChild(li)});
}

/* 「传书」固定 tab = 三层架构入口：入库（所有内容源汇入）｜母版库（可选优化 → 选去向落库）。放第一位。
   读器页（xochitl / KOReader）不再有任何传书入口，只管各自的字体 / 词典。 */
function renderTransfer(sec){sec.innerHTML=`
  <div class="subnav"><button class="on">📥 入库</button><button>📚 母版库</button></div>
  <div class="subpanel on">
    <div class="card"><h2>传书 · 入库</h2>
      <p class="lead">所有书从这里进：上传、抓网文、微信读书、电脑 shelf push。原样入库、不动字节；洗不洗、放哪读，到「母版库」再定。</p>
      ${GUIDE}
      ${onUsb?'':'<p class="opt-note">传大书建议走 USB <code>https://10.11.99.1:8778</code>，不占 Wi-Fi。</p>'}
      <h3>上传</h3>
      ${upHtml('⬆','点击或拖入书（可多选 · '+extLabel(BOOK_EXT)+'）',BOOK_EXT,'进母版库')}
      <p class="small">EPUB / PDF 两个读器都能去；其它格式只能加入 KOReader，想进原生用电脑 <code>shelf push</code> 转成 EPUB。不是书的文件（图片 / 压缩包）不收。</p>
      <h3>抓网文</h3>
      <div class="row"><input type="text" id="arturl" placeholder="https://… 文章链接（公众号 / 博客 / 新闻）" style="flex:1;min-width:12em"><button class="btn" id="artgo">抓取进母版库</button></div>
      <div class="small" id="artmsg" style="margin-top:.3em"></div>
      <p class="small">静态网页效果好；纯 JS 页面、付费墙抓不出。单篇文章（连载分章后续）。</p>
      <h3>微信读书 <span class="badge">即将接入</span></h3>
      <p class="small">扫码登录 → 选书 → 下成 EPUB 进母版库，再选读器读（原生阅读器体验远好于网页版）。</p>
    </div>
  </div>
  <div class="subpanel">
    <div class="card"><h3 style="margin-top:0">母版库 <span class="small" id="stgcap"></span></h3>
      <div class="row"><span class="small">投原生 → 文件夹</span><select id="folderPreset" style="max-width:13em"><option value="lib">书库（默认）</option><option value="annot">批注文件夹</option><option value="custom">自定义…</option></select><input type="text" id="folder" placeholder="文件夹名" style="display:none;max-width:10em">
        <span class="small">加入 KOReader → 目录</span><input type="text" id="kfolder" list="kodirs" placeholder="留空＝根目录" style="max-width:9em"><datalist id="kodirs"></datalist></div>
      <div class="row"><span class="small">优化档位</span><select id="optmode" style="max-width:15em"><option value="auto">清洗＋优化（推荐）</option><option value="keep-spacing">清洗但保留段距（诗集 / 剧本）</option><option value="plain">只优化不清洗</option></select>
        <label class="toggle"><input type="checkbox" id="stgclear"> 投完从母版库清除</label></div>
      <details class="cmp"><summary>档位说明 · 母版为什么默认保留</summary>${OPTTABLE}<p class="small">母版保留＝同一本可再投另一个读器对照、换设备重投；不需要了手动删。</p></details>
      <div class="row"><input type="text" id="stgq" placeholder="搜书名…" style="flex:1;min-width:8em"><select id="stgfmt" style="max-width:8em"><option value="">全部格式</option><option value="epub">EPUB</option><option value="pdf">PDF</option><option value="other">其它</option></select><select id="stgst" style="max-width:8em"><option value="">全部状态</option><option value="0">未优化</option><option value="1">已优化</option></select><button class="btn" id="stgpurge" title="删除已投过读器的母版（读器里的书不受影响）">清理已落库</button></div>
      <div class="small" id="stgfree" style="margin:-.3em 0 .4em"></div>
      <ul class="list" id="stglist"></ul>
    </div>
  </div>`;
  let annotFolder='',koInstalled=false,items=[];
  const g=id=>$('#'+id,sec);
  const xFolder=()=>{const p=g('folderPreset').value;return p==='lib'?'':p==='annot'?annotFolder:g('folder').value.trim()};
  const syncFolder=()=>{g('folder').style.display=g('folderPreset').value==='custom'?'':'none'};
  // 落库设置记在本机（per-viewer 便利态）
  [['folderPreset','fpreset','lib'],['folder','folder',''],['kfolder','kfolder',''],['optmode','optmode','auto']].forEach(([id,k,d])=>{g(id).value=LS.get(k,d);['input','change'].forEach(ev=>g(id).addEventListener(ev,()=>{LS.set(k,g(id).value);if(id==='folderPreset')syncFolder()}))});
  g('stgclear').checked=LS.get('stgclear','0')==='1';g('stgclear').onchange=()=>LS.set('stgclear',g('stgclear').checked?'1':'0');
  syncFolder();
  const render=()=>stagingList(g('stglist'),{items,q:g('stgq').value,fmt:g('stgfmt').value,st:g('stgst').value,xFolder,kFolder:()=>g('kfolder').value.trim(),mode:()=>g('optmode').value,clear:()=>g('stgclear').checked,koInstalled,refresh:()=>refresh()});
  ['stgq','stgfmt','stgst'].forEach(id=>['input','change'].forEach(ev=>g(id).addEventListener(ev,render)));
  const refresh=async()=>{const [d,s,k,kb]=await Promise.all([j('/api/books/staging'),j('/api/books/status'),j('/api/koreader/status'),j('/api/koreader/books')]);
    if(s.ok&&s.annotFolder)annotFolder=s.annotFolder;koInstalled=!!(k.ok&&k.installed);
    // KOReader 现有目录 → 下拉候选（免手打错）
    g('kodirs').innerHTML=(kb.items||[]).filter(x=>x.kind==='dir').map(x=>`<option value="${x.name}">`).join('');
    if(d.ok===false){g('stglist').innerHTML=`<li class="small" style="color:var(--bad)">母版库不可用：${d.message||'book-serve 未开'}（去「管理」页开启）</li>`;g('stgcap').textContent='';return}
    items=d.items||[];const tot=items.reduce((a,b)=>a+b.bytes,0);g('stgcap').textContent=items.length?`${items.length} 本 · ${fmtB(tot)}`:'';
    const fr=d.freeBytes;const low=fr!=null&&fr<300*1048576;g('stgfree').style.color=low?'var(--bad)':'';g('stgfree').textContent=fr!=null?`设备剩余空间 ${fmtB(fr)}${low?' ⚠ 快满了：清理已落库或删不要的母版':''}`:'';
    render()};
  g('stgpurge').onclick=async()=>{const done=items.filter(it=>it.delivered&&(it.delivered.native||it.delivered.koreader));if(!done.length){alert('没有已落库的母版');return}
    if(!confirm(`删除 ${done.length} 本已投过读器的母版？（读器里的书不受影响，只是不能再重投）`))return;
    for(const it of done)await postJ('/api/books/staging/delete',{name:it.name});refresh()};
  uploader($('.up',sec),()=>'/api/books/staging',()=>({}),BOOK_EXT,()=>refresh());   // 书籍格式原样入库；选中即按 BOOK_EXT 拦
  const am=g('artmsg'),au=g('arturl'),ag=g('artgo');
  ag.onclick=async()=>{const url=au.value.trim();if(!url){am.textContent='请填链接';return}ag.disabled=true;am.style.color='';am.textContent='抓取中…（联网抽取正文，十几秒）';
    const r=await j('/api/books/staging/fetch-article',{method:'POST',body:JSON.stringify({url})});ag.disabled=false;
    am.style.color=r.ok===false?'var(--bad)':'var(--ok)';am.textContent=r.ok===false?('✗ '+(r.message||'失败')):('✓ '+r.message);if(r.ok!==false){au.value='';refresh()}};
  refresh();sec.refresh=refresh;subtabs(sec);}

/* 服务 tab（按注册表出现）。key = 注册的服务名 */
const TABS={
 'font-serve':{title:'xochitl',render(sec){assetTab(sec,'/api/fonts',{
   title:'xochitl · 原生字体',
   hint:'ttf / otf → 装进 fontconfig 用户字体目录。上传后阅读器「文字与布局」菜单重开即可选，无需重启。传书在「传书」页；KOReader 的字体在 KOReader 页装。',
   header:`<div id="fbchain" class="opt-note" style="display:none"></div>
      <div class="row"><label class="toggle"><input type="checkbox" id="embold" checked> 中文加粗（墨水屏细笔画补偿）</label> <span class="small">默认开：对回退中文字体加粗，宋体在低对比墨水屏发淡时更清楚；翻书即见。</span></div>`,
   icon:'🔤',label:'点击或拖入 ttf/otf（可多选）',accept:FONT_EXT,btn:'上传字体',listTitle:'已装字体',
   onRender:async(sec,refresh,fl)=>{
     // 中文缺字回退链：覆盖率≥8% 的中文字体，按覆盖率降序
     const cjk=(fl.items||[]).filter(it=>((it.extra||{}).cjkPct||0)>=8).sort((a,b)=>(b.extra.cjkPct||0)-(a.extra.cjkPct||0));
     const fb=$('#fbchain',sec);fb.style.display='';fb.innerHTML=cjk.length?`中文缺字回退：${cjk.map(it=>`${it.name} <span class="small">${it.extra.cjkPct}%</span>`).join(' → ')}`:'⚠ 未装中文字体，正文缺字会显示方框——传一个全覆盖中文字体即可兜底。';
     const fst=await j('/api/fonts/status');const eb=$('#embold',sec);if(fst.ok){eb.checked=!!fst.emboldenCjkFallback;eb.onchange=async()=>{const r=await j('/api/fonts/config',{method:'PUT',body:JSON.stringify({emboldenCjkFallback:eb.checked})});if(r.ok===false){alert(r.message);eb.checked=!eb.checked}}}},
   row:(it,left,right,refresh)=>{const ex=it.extra||{};
     left.innerHTML=`${it.name}${ex.names&&ex.names.cn&&ex.names.cn!==it.name?' <span class="small">'+ex.names.cn+'</span>':''}${ex.files&&ex.files.length>1?' <span class="small">×'+ex.files.length+'</span>':''}`;
     right.insertAdjacentHTML('beforeend',cjkBadge(ex.cjkPct)+(ex.fontconfigRef?'<span title="界面中文回退引用">⚠</span>':''));
     right.appendChild(delBtn('删除字体 '+it.name+(ex.files&&ex.files.length>1?'（含 '+ex.files.length+' 个文件）':'')+(ex.fontconfigRef?'？\n⚠ 该字体是界面中文回退字体':'？'),'/api/fonts/'+encodeURIComponent(it.name),refresh))}})}},
 'koreader-serve':{title:'KOReader',render(sec){sec.innerHTML=`
  <div class="subnav"><button class="on">🔤 字体</button><button>📖 词典</button></div>
  <div class="subpanel on">
    <div class="card"><h2>KOReader</h2><div class="kv small" id="ks" style="margin-top:.5em">加载…</div>
      <details class="cmp"><summary>KOReader 怎么装 / 已经帮你调好了什么</summary>
      <dl class="help">
        <dt>安装</dt><dd>走官方仓库自装：先备齐基石 xovi + appload（见「管理」页），再从 <a href="https://github.com/koreader/koreader/releases" target="_blank" rel="noopener">官方 releases</a> 装 reMarkable Paper Pro（rmpp）版。</dd>
        <dt>已按 Move 屏调好（开箱即用，不用手动配）</dt><dd>本套件的 profile 贴近 xochitl 观感：中文主字体霞鹜新致宋、页边距、行距、脚注<b>底部弹窗</b>、悬挂标点、防误触、退出手势。</dd>
        <dt>书从哪来</dt><dd>在「传书」页把书入母版库，点「加入 KOReader」即可（${extLabel(BOOK_EXT)}）。有什么书，去 KOReader 里看。</dd>
        <dt>改配置 / 删字体后</dt><dd>KOReader 若正在跑，需<b>重启它</b>才生效（上方状态「运行中」会提示）。</dd>
      </dl></details></div>
    <div class="card"><h3 style="margin-top:0">字体（KOReader）</h3><p class="small">只装进 KOReader；原生阅读器的字体在 xochitl 页装。</p>
    ${upHtml('🔤','点击或拖入 ttf/otf（可多选）',FONT_EXT,'上传字体')}
    <h3>已装字体</h3><ul class="list" id="kf"></ul></div>
  </div>
  <div class="subpanel">
    <div class="card"><h3 style="margin-top:0">词典（KOReader）</h3><p class="small">StarDict 词典：填词典名，拖入这本词典的全部文件（${DICT_EXT.join(' / ')}）一起传。</p>
    <label class="field" for="dictname">词典名</label><input type="text" id="dictname" placeholder="如 牛津高阶 / cc-cedict">
    ${upHtml('📖','点击或拖入词典文件（可多选）',DICT_EXT,'上传词典')}
    <h3>已装词典</h3><ul class="list" id="kd"></ul></div>
  </div>`;
  const ups=sec.querySelectorAll('.up');
  uploader(ups[0],()=>'/api/koreader/fonts',()=>({}),FONT_EXT,()=>refresh());
  uploader(ups[1],()=>'/api/koreader/dicts',()=>({name:$('#dictname',sec).value.trim()}),DICT_EXT,()=>refresh());
  const refresh=async()=>{
    const [s,f,dc]=await Promise.all([j('/api/koreader/status'),j('/api/koreader/fonts'),j('/api/koreader/dicts')]);
    $('#ks',sec).innerHTML=s.ok?`<b>安装</b><span>${s.installed?'是':'否'} ${s.version?'('+s.version+')':''}</span><b>运行中</b><span>${s.running?'是（改配置 / 删字体后需重启它）':'否'}</span><b>已装</b><span>字体 ${s.fonts} 个 · 词典 ${s.dicts||0} 本</span>`:`<span>${s.message}</span>`;
    fillList($('#kf',sec),f.items||[],(it,left,right)=>{left.textContent=it.name;right.insertAdjacentHTML('beforeend',cjkBadge(it.cjkPct)+`<span>${fmtB(it.bytes)}</span>`);right.appendChild(delBtn('从 KOReader 删除 '+it.name+'？','/api/koreader/fonts/'+encodeURIComponent(it.name),refresh))});
    fillList($('#kd',sec),dc.items||[],(it,left,right)=>{left.textContent='📖 '+it.name;right.textContent=it.ifo+' 本'})};
  refresh();sec.refresh=refresh;subtabs(sec)}},
 'wallpaper-serve':{title:'壁纸',render(sec){assetTab(sec,'/api/wallpapers',{
   hint:'jpg / png 图片，自动裁到 954×1696。首张自动启用，下次休眠即生效。',
   header:`<label class="field">休眠轮换</label><div class="row"><select id="wpmode" style="max-width:12em"><option value="sequential">按顺序</option><option value="random">随机</option><option value="fixed">固定</option></select><span id="wpst" class="small"></span></div>`,
   icon:'🖼',label:'点击或拖入图片（可多选）',accept:IMG_EXT,btn:'上传',
   onRender:async(sec,refresh)=>{const st=await j('/api/wallpapers/status');const sel=$('#wpmode',sec);if(st.ok){sel.value=st.mode;$('#wpst',sec).textContent=`当前 ${st.current||'（无）'} · 已挂载 ${st.mounted}/${st.expectedMounts}`}
     sel.onchange=async()=>{await j('/api/wallpapers/mode',{method:'PUT',body:JSON.stringify({mode:sel.value})});refresh()}},
   row:(it,left,right,refresh)=>{const cur=(it.extra||{}).current;
     left.innerHTML=`<img src="/api/wallpapers/${encodeURIComponent(it.name)}" alt="" style="height:3.4em;border-radius:.3em;border:1px solid var(--line);margin-right:.6em;vertical-align:middle">${it.name}`;
     right.insertAdjacentHTML('beforeend',`<span>${fmtB(it.bytes)}</span>`+(cur?'<span class="badge on">当前</span>':''));
     if(!cur){const b=document.createElement('button');b.className='btn';b.textContent='使用';b.onclick=async()=>{await j('/api/wallpapers/current',{method:'PUT',body:JSON.stringify({name:it.name})});refresh()};right.appendChild(b);
       right.appendChild(delBtn('删除 '+it.name+'？','/api/wallpapers/'+encodeURIComponent(it.name),refresh))}}})}}
 // 微读不再是独立 tab：定位为「传书·入库」的一种内容源（下书→EPUB→母版库），Phase D 接入。
};

/* 资产页模板（字体 / 壁纸）：说明 + 可选头部 + 上传区 + 列表。o: {title?,hint,header?,icon,label,accept,btn,listTitle?,onRender?(sec,refresh,data),row(it,left,right,refresh)} */
function assetTab(sec,api,o){sec.innerHTML=`<div class="card">${o.title?`<h2>${o.title}</h2>`:''}<p class="${o.title?'small':'lead'}">${o.hint}</p>${o.header||''}
  ${upHtml(o.icon,o.label,o.accept,o.btn)}
  <h3>${o.listTitle||'已安装'}</h3><ul class="list" id="al"></ul></div>`;
  const refresh=async()=>{const d=await j(api);fillList($('#al',sec),d.items||[],(it,left,right)=>o.row(it,left,right,refresh));if(o.onRender)o.onRender(sec,refresh,d)};
  uploader($('.up',sec),()=>api,()=>({}),o.accept,refresh);
  refresh();sec.refresh=refresh}

/* 管理台/引导（固定 tab，始终在——它是网关自身页面，不由服务注册表驱动） */
function renderManage(sec){sec.innerHTML=`
  <div class="card"><h2>引导 · 基石</h2><p class="lead">书架的功能建在 xovi + appload 之上。先用桌面端 <b>reManager</b>（或设备上的 vellum）把基石装好，KOReader 走官方仓库自装，再回这里管理书架各功能。</p>
    <div class="kv small" id="found">检测中…</div>
    <p class="small">下载 / 文档：<a href="https://github.com/rmitchellscott/reManager" target="_blank" rel="noopener">reManager</a>（桌面端 · vellum 生态）· <a href="https://github.com/asivery/rmpp-xovi" target="_blank" rel="noopener">xovi</a> · <a href="https://github.com/koreader/koreader/wiki" target="_blank" rel="noopener">KOReader Wiki</a></p></div>
  <div class="card"><h2>电脑端 · <code>shelf push</code>（进阶洗书 / PDF 重排）</h2>
    <p class="lead">难搞的书用它：非标准格式转 EPUB、Calibre 级深洗、PDF 论文重排——网页直传做不到的都在这。</p>
    <div class="opt-note">
      <b>命令长这样</b>（在本仓库目录下跑；<code>shelf/host/bin/shelf</code> 就是那个命令，嫌长可 <code>alias shelf="$PWD/shelf/host/bin/shelf"</code>）：<br>
      <code>shelf/host/bin/shelf push &lt;书1&gt; [书2 …]</code>
      <div class="small" style="margin-top:.4em">
        · 后面只跟<b>要投的书</b>（可一次多本）；<b>没有输出路径、也没有目标参数</b>——洗完一律落到<b>母版库</b>，放哪个读器你在网页「传书 → 母版库」里点。<br>
        · 有 Calibre 就先洗（EPUB 深洗 / 杂格式转 EPUB / PDF 结构化重排）；<code>--no-optimize</code> 不洗原样传；<code>--to-pdf</code> 定稿成手写批注用的 PDF。<br>
        · 和网页规则一致：<b>所有书只落母版库</b>，没有直投读器的选项。
      </div>
    </div>
    <details class="cmp"><summary>例子 / 强在哪 / 怎么装</summary>
    <dl class="help">
      <dt>例子</dt>
      <dd><code>shelf/host/bin/shelf push 论文.pdf</code> — PDF 结构化重排 → 母版库，再到「传书 → 母版库」选去向<br>
          <code>shelf/host/bin/shelf push 小说.azw3</code> — 转干净 EPUB → 母版库（网页里点「加入 KOReader」）<br>
          <code>shelf/host/bin/shelf push 书.epub --to-pdf</code> — 定稿固定版式 PDF → 母版库（投 xochitl 手写批注）<br>
          <code>shelf/host/bin/shelf push a.epub b.mobi</code> — 一次多本<br>
          <code>shelf/host/bin/shelf status</code> · <code>doctor</code> — 看设备连通 / 环境</dd>
      <dt>强在哪</dt>
      <dd>① <b>杂格式转干净 EPUB</b>：AZW3 / MOBI / FB2 / TXT… 网页端不收，这里能转；② <b>Calibre 级深洗</b>：CSS 拍平比端上更彻底，排版锁死的书也能救；③ <b>PDF 论文重排</b>：多列 / 公式 / 图按阅读顺序重排到屏宽——<b>端上做不到</b>（端上 PDF 只原样直传）；④ 扫描件走 k2pdfopt。产物再叠加设备同款优化器，观感与网页直传一致。</dd>
      <dt>怎么装</dt>
      <dd>电脑装 <a href="https://calibre-ebook.com/" target="_blank" rel="noopener">Calibre</a>（含 ebook-convert）+ Python 3 → 克隆本仓库 → <code>cd shelf &amp;&amp; sh build.sh</code>（编出 <code>epub-optimize</code>）→ 就能用 <code>shelf/host/bin/shelf</code> 了。</dd>
    </dl></details></div>
  <div class="card"><h2>书架功能</h2>
    <p class="lead">每个功能可单独<b>开关</b>、<b>卸载</b>；未装的按命令安装。网关（本页）始终在。</p>
    <details class="cmp"><summary>三态 / 开关 / 卸载 / 安装 是什么？（点开看说明）</summary>
      <dl class="help">
        <dt>三种状态</dt>
        <dd><span class="badge off">未装</span> 设备上没这个程序 → 按给出的命令安装。<br>
            <span class="badge">已装·未开</span> 程序在、后台没跑 → 网页看不到它的功能，点「开启」启用。<br>
            <span class="badge on">已开</span> 后台在跑 → 顶部有它的标签页，功能可用。</dd>
        <dt>开启 / 关闭</dt>
        <dd><b>关闭＝只停后台服务</b>：网页隐藏该标签，但<b>已经生效的东西照常用</b>——已装字体仍能在阅读器里选、壁纸仍显示、KOReader 仍能打开；只是不能再用网页传 / 改它。用途是隐藏用不到的功能、减少对外暴露面。</dd>
        <dt>常开会不会卡 / 费电？</dt>
        <dd>不会。实测 5 个服务全部常开共约 <b>9 MB 内存</b>、开机一整天累计不到 <b>2 秒 CPU</b>（平均约 0.002%），平时都阻塞在等请求、不抢 CPU。<b>对看书 / 记笔记零可感影响，不卡、不额外费电。</b>建议全部常开，除非某功能你确定永远不用。</dd>
        <dt>卸载</dt>
        <dd>删掉该功能的程序、systemd 单元和相关注入文件（qmd）。<b>你传过的书 / 字体 / 壁纸等用户数据保留。</b>卸载后它从网页消失；想再用按安装命令重装。网关不能从网页关或卸——它是本管理页的宿主。</dd>
        <dt>安装为什么不在网页做？</dt>
        <dd>安装要重挂载只读系统分区、写系统单元，风险偏高。<b>未装功能只给命令</b>：在电脑上 SSH 跑，或走 reManager 引导，更安全。</dd>
      </dl></details>
    <div class="row"><button class="btn" id="allon">全部开启</button><button class="btn" id="alloff">全部关闭（留网关）</button></div>
    <ul class="list" id="mods"></ul></div>`;
  const badge=(t,ok)=>`<span class="badge ${ok?'on':'off'}">${t}</span>`;
  const refresh=async()=>{
    const f=await j('/api/foundation');$('#found',sec).innerHTML=f.ok===false?`<span>${f.message}</span>`:
      `<b>xovi</b><span>${badge(f.xovi?'已装':'未装',f.xovi)}</span><b>appload</b><span>${badge(f.appload?'已装':'未装',f.appload)}</span><b>qt-resource-rebuilder</b><span>${badge(f.qrr?'已装':'未装',f.qrr)}</span><b>KOReader</b><span>${badge(f.koreader?'已装':'未装',f.koreader)}</span>`;
    const d=await j('/api/manage');const ul=$('#mods',sec);ul.innerHTML='';(d.modules||[]).forEach(m=>{const li=document.createElement('li');li.style.flexWrap='wrap';
      let state,cls;if(!m.installable){state='未上线';cls=''}else if(!m.installed){state='未装';cls='off'}else if(m.running){state='已开';cls='on'}else{state='已装·未开';cls=''}
      const left=document.createElement('span');left.innerHTML=`${m.label} <span class="small">${m.service}</span> <span class="badge ${cls}">${state}</span>`;
      const right=document.createElement('span');right.style.cssText='display:flex;gap:.4em;align-items:center';
      if(m.installable&&m.installed){
        const t=document.createElement('button');t.className='btn';t.textContent=m.running?'关闭':'开启';
        t.onclick=async()=>{const r=await j('/api/manage/'+m.seg+'/'+(m.running?'stop':'start'),{method:'POST'});if(r.ok===false)alert(r.message);setTimeout(refresh,600)};right.appendChild(t);
        const u=document.createElement('button');u.className='btn';u.textContent='卸载';
        u.onclick=async()=>{if(confirm('卸载 '+m.label+'？删除它的服务/单元/相关 qmd（用户数据保留）。')){const r=await j('/api/manage/'+m.seg+'/uninstall',{method:'POST'});if(r.ok===false)alert(r.message);else alert('已卸载 '+m.label);setTimeout(()=>location.reload(),800)}};right.appendChild(u);
      }else if(m.installable){const g=document.createElement('span');g.className='small';g.innerHTML='装：<code>shelf/install.sh --only '+m.only+'</code>';right.appendChild(g)}
      li.append(left,right);ul.appendChild(li)});};
  $('#allon',sec).onclick=async()=>{const d=await j('/api/manage');for(const m of (d.modules||[]))if(m.installable&&m.installed&&!m.running)await j('/api/manage/'+m.seg+'/start',{method:'POST'});refresh()};
  $('#alloff',sec).onclick=async()=>{if(!confirm('关闭全部领域服务（网关保留）？'))return;const d=await j('/api/manage');for(const m of (d.modules||[]))if(m.installable&&m.installed&&m.running)await j('/api/manage/'+m.seg+'/stop',{method:'POST'});refresh()};
  refresh();sec.refresh=refresh;}

(async()=>{const d=await j('/api/services');const svcs=(d.services||[]).filter(s=>s.ui&&TABS[s.name]).sort((a,b)=>a.ui.order-b.ui.order);
  $('#hdr').textContent=location.host;
  const nav=$('#tabs'),main=$('#main');main.innerHTML='';
  const addTab=(title,render,first)=>{const b=document.createElement('button');b.textContent=title;const sec=document.createElement('section');
    b.onclick=()=>{[...nav.children].forEach(x=>x.classList.remove('on'));[...main.children].forEach(x=>x.classList.remove('on'));b.classList.add('on');sec.classList.add('on');if(sec.refresh)sec.refresh()};
    nav.appendChild(b);main.appendChild(sec);render(sec);if(first)b.onclick()};
  addTab('传书',renderTransfer,true);          // 总入口（入库｜母版库），固定第一位（book-serve 不在时列表里提示去管理页开）
  svcs.forEach((s)=>addTab(TABS[s.name].title,TABS[s.name].render,false));
  addTab('管理',renderManage,false);            // 固定管理台，始终可进
})();
</script></body></html>
"##;


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

#[cfg(test)]
mod tests {
    #[test]
    fn page_injects_format_whitelists_once() {
        let p = super::page();
        assert!(!p.contains("__EXTS__"), "占位应被替换");
        assert!(p.contains(r#""book":["epub","pdf""#) && p.contains(r#""font":["ttf""#) && p.contains(r#""dict":["ifo""#) && p.contains(r#""image":["jpg""#));
        assert!(std::ptr::eq(p, super::page()), "OnceLock 只渲染一次");
    }
}
