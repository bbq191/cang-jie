//! 单页 UI（手机/电脑浏览器打开 `http://<设备IP>:8778/`）。tab 按 `/api/services` 动态生成；
//! 传书 tab 的目标下拉按 target 打不同路由（native/annot → /api/books，koreader → /api/koreader/books）。
//! 上传逐文件一请求（每本独立成败、独立进度条）；字体/壁纸 tab 复用同一上传器（AssetUploadFlow 回执同形）。
pub const PAGE: &str = r##"
<!doctype html><html lang="zh"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>书架</title>
<style>
:root{color-scheme:light dark;
 --bg:#f6f7f9;--surface:#fff;--surface2:#f0f2f5;--fg:#1a1c1e;--mute:#5b6169;--line:#dfe3e8;--line2:#eceef1;
 --accent:#3257d0;--accent-fg:#fff;--ok:#1a7f37;--bad:#c0362c;--warn:#9a6700;--shadow:0 1px 2px rgba(0,0,0,.05),0 4px 14px rgba(0,0,0,.05)}
@media (prefers-color-scheme:dark){:root{
 --bg:#16181c;--surface:#1e2126;--surface2:#262a30;--fg:#e6e8eb;--mute:#9aa2ad;--line:#333941;--line2:#2a2f36;
 --accent:#7d9bff;--accent-fg:#10131a;--ok:#5fce7f;--bad:#ff8c80;--warn:#e3b341;--shadow:0 1px 2px rgba(0,0,0,.4),0 6px 18px rgba(0,0,0,.35)}}
*{box-sizing:border-box}
body{font-family:system-ui,-apple-system,"PingFang SC","Noto Sans CJK SC",sans-serif;margin:0;color:var(--fg);background:var(--bg);line-height:1.5}
a{color:var(--accent)}
header{position:sticky;top:0;z-index:5;background:color-mix(in srgb,var(--surface) 88%,transparent);backdrop-filter:blur(8px);border-bottom:1px solid var(--line);padding:.7em 1.1em;display:flex;align-items:center;gap:.8em}
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
$('#logout').onclick=e=>{e.preventDefault();fetch('/logout',{method:'POST'}).then(()=>location.href='/login')};
async function j(url,opt){const r=await fetch(url,opt);if(r.status===401){location.href='/login?next='+encodeURIComponent(location.pathname);return {ok:false,message:'未登录'}}if(r.status===403){location.href='/password';return {ok:false,message:'需先改密码'}}let d;try{d=await r.json()}catch{d={ok:false,message:'HTTP '+r.status}}if(!r.ok&&d.ok!==false)d={ok:false,message:d.message||('HTTP '+r.status)};return d}

/* 通用上传器：逐文件一请求，进度条，逐项回执；失败项可重传，队列可逐项删/清空，顶部总进度 */
function uploader(box,urlOf,queryOf,okExt){
  const list=$('ul.q',box), input=$('input[type=file]',box), drop=$('.drop',box), go=$('.go',box);
  let files=[], sum=null;
  // 自动补一个"清空"按钮（各上传器统一，不必每处 HTML 写）
  const clr=document.createElement('button');clr.type='button';clr.className='btn';clr.textContent='清空';clr.onclick=()=>{files=[];render()};go.after(clr);
  const summary=()=>{if(!sum){sum=document.createElement('div');sum.className='small';sum.style.margin='.3em 0';list.parentNode.insertBefore(sum,list)}
    const ok=files.filter(f=>f.st==='ok').length,bad=files.filter(f=>f.st==='bad').length;
    sum.innerHTML=files.length?`${ok}/${files.length} 完成${bad?` · <span style="color:var(--bad)">${bad} 失败</span>`:''}`:'';};
  const render=()=>{list.innerHTML='';files.forEach(f=>{const li=document.createElement('li');li.dataset.k=f.k;li.className=f.st||'';
      li.innerHTML=`<div class="name">${f.file.name} <span class="small">${fmtB(f.file.size)}</span> <button class="btn x" type="button" title="移除" style="padding:.05em .45em;line-height:1">×</button></div><progress value="${f.st==='ok'?100:0}" max="100"></progress><div class="msg">${f.msg||'待传'}</div>`;
      li.querySelector('.x').onclick=()=>{files=files.filter(x=>x.k!==f.k);render()};list.appendChild(li)});summary()};
  const add=fl=>{for(const f of fl){const rej=okExt&&!okExt.some(e=>f.name.toLowerCase().endsWith(e));
      files.push({file:f,k:Math.random().toString(36).slice(2),rej,st:rej?'bad':'',msg:rej?('格式不收：只接受 '+okExt.join(' / ')+'；其它格式请在电脑用 shelf push 转换'):''})}render()};
  input.onchange=()=>{add(input.files);input.value=''};
  drop.ondragover=e=>{e.preventDefault();drop.classList.add('hi')};drop.ondragleave=()=>drop.classList.remove('hi');
  drop.ondrop=e=>{e.preventDefault();drop.classList.remove('hi');add(e.dataTransfer.files)};
  drop.onclick=()=>input.click();
  go.onclick=async()=>{go.disabled=true;clr.disabled=true;
    for(const f of files){if(f.st==='ok'||f.rej)continue;          // 成功项跳过；格式不收项不上传；失败项允许重传（修 X1）
      const li=list.querySelector(`li[data-k="${f.k}"]`);if(!li)continue;const pg=$('progress',li),msg=$('.msg',li);
      f.st='';li.className='';pg.value=0;msg.textContent='上传中…';
      await new Promise(res=>{const x=new XMLHttpRequest();const q=queryOf();x.open('POST',urlOf()+(q?'?'+new URLSearchParams(q):''));
        x.upload.onprogress=e=>{if(e.lengthComputable)pg.value=e.loaded/e.total*100};
        x.onload=()=>{if(x.status===401){location.href='/login';return}let d;try{d=JSON.parse(x.responseText)}catch{d={ok:false,message:'HTTP '+x.status}}
          const it=(d.items&&d.items[0])||d;f.st=it.ok?'ok':'bad';f.msg=(it.message||(it.ok?'完成':'失败'))+(d.note&&it.ok?' · '+d.note:'');li.className=f.st;msg.textContent=f.msg;pg.value=100;summary();res()};
        x.onerror=()=>{f.st='bad';f.msg='网络错误';li.className='bad';msg.textContent=f.msg;summary();res()};
        const fd=new FormData();fd.append('file',f.file);x.send(fd)})}
    go.disabled=false;clr.disabled=false};
  return {clear(){files=[];render()}};
}

/* 二级标签：面板都由外层 render/refresh 预先填好，切换只显隐（复用主 addTab 的思路，不重复请求） */
function subtabs(sec){const nav=$('.subnav',sec);if(!nav)return;const btns=[...nav.children],panels=[...sec.querySelectorAll('.subpanel')];
  btns.forEach((b,i)=>b.onclick=()=>{btns.forEach(x=>x.classList.remove('on'));panels.forEach(p=>p.classList.remove('on'));b.classList.add('on');if(panels[i])panels[i].classList.add('on')});}

const CMP=`<details class="cmp"><summary>三种读法怎么选？（点开对照）</summary>
<div class="tblwrap"><table class="cmp"><thead><tr><th></th><th>📖 xochitl 原生</th><th>📚 KOReader</th><th>💻 电脑 shelf push</th></tr></thead><tbody>
<tr><th>适合</th><td>学术 / 要做笔记的书：目录、脚注、换字体、直接手写批注</td><td>消遣阅读：重排、内置词典、翻页手势，小说漫画顺手</td><td>难搞的书：非标准格式、要深洗、PDF 论文要重排</td></tr>
<tr><th>怎么用</th><td class="pick">就在本页「传书」拖文件</td><td>见上方 KOReader 标签页</td><td>电脑装 Calibre，命令行 <code>shelf push 书</code></td></tr>
<tr><th>收什么</th><td>EPUB、PDF <span class="small">（AZW3/MOBI/CBZ 等走电脑端转换）</span></td><td>任意格式，原样</td><td>任意：EPUB 深洗、PDF 重排、杂格式转 EPUB</td></tr>
<tr><th>动文件吗</th><td>EPUB 自动优化（可选档位）；<b>PDF 原样</b>（端上不重排）</td><td class="pick">完全不动，字节不改</td><td>Calibre 级清洗 + 端同款优化；PDF 结构化重排</td></tr>
<tr><th>能调字体字号</th><td class="pick">EPUB 能（reMarkable 字体设置生效）；PDF 不能</td><td class="pick">能（KOReader 自己排版）</td><td>产物投到上面两处后同理</td></tr>
</tbody></table></div>
<h3 style="margin:.8em 0 .2em">EPUB 处理档位（PDF 不受影响，永远原样投）</h3>
<div class="tblwrap"><table class="cmp"><thead><tr><th>档位</th><th>做什么</th><th>什么时候用</th></tr></thead><tbody>
<tr><th class="pick">清洗+优化<br><span class="small">默认</span></th><td>全套：剥字体/字号/颜色/对齐锁 · 边距段距归零+按中英文习惯首行缩进 · 缺目录按标题自动建 · 伪 DRM 剥离 · 脚注就地内联常显 · 远程图内联 · 图片降采样 · e-ink 提对比 · 双 id 去重</td><td>绝大多数第三方书、网上下载的书</td></tr>
<tr><th>清洗但保留段距</th><td>同上，但不动原书段间距</td><td>诗集 / 剧本 / 靠空行分节的书</td></tr>
<tr><th>只优化不清洗</th><td>只修脚注 / 图片 / 对比度 / 双 id，不碰排版和字体锁</td><td>已排好版、只想修脚注和图的书</td></tr>
<tr><th>原样进库</th><td>什么都不做</td><td>你确定这本已经很完美</td></tr>
</tbody></table></div>
<p class="opt-note"><b>质量门</b>（仅对 EPUB）：真 DRM / 目录损坏 / 双 id 非法（会让 reMarkable 整章白屏）这三类会被<b>硬拦</b>，不投递并说明原因。取消勾选＝跳过检查强行投递。更高质量的洗书 / PDF 重排在电脑用 <code>shelf push</code>（见「管理」页说明），本页不调用 Calibre。</p>
</details>`;

const TABS={
 'book-serve':{title:'xochitl',render(sec){sec.innerHTML=`
  <div class="subnav"><button class="on">📚 传书</button><button>🔤 原生字体</button></div>
  <div class="subpanel on">
    <div class="card">
      <h2>传书到 xochitl</h2><p class="lead">拖文件进来就行：EPUB 自动优化排版，PDF 原样进设备（两者都能手写批注）。</p>
      ${onUsb?'':'<p class="opt-note">传大书建议走 USB：设备插线后开 <code>https://10.11.99.1:8778</code>，不占 Wi-Fi（每本书 xochitl 会往云端同步，走弱热点会卡）。</p>'}
      ${CMP}
      <label class="field" for="folderPreset">放进哪个文件夹</label>
      <select id="folderPreset"><option value="lib">书库（默认）</option><option value="annot">批注文件夹（PDF 手写定稿常放这）</option><option value="custom">自定义…</option></select>
      <input type="text" id="folder" placeholder="文件夹名，如 论文/2026" style="display:none;margin-top:.4em">
      <label class="field">EPUB 处理 <span class="small" style="font-weight:400">— PDF 不受影响，永远原样投</span></label>
      <select id="opt"><option value="auto">清洗 + 优化（推荐 · 剥字体锁、归零边距、按中英文习惯缩进、缺目录自动建）</option><option value="keep-spacing">清洗但保留段距（诗集 / 剧本）</option><option value="plain">只优化不清洗（脚注 / 图片 / 对比度）</option><option value="off">原样进库</option></select>
      <div class="row" style="margin:.5em 0 0"><label class="toggle"><input type="checkbox" id="chk" checked> 质量门（真 DRM / 目录坏 / 双 id 硬拦，仅对 EPUB）</label></div>
      <p class="opt-note">PDF 想<b>重排</b>（多列论文、公式、扫描件按屏宽重排）请在电脑用 <code>shelf push 论文.pdf</code>——端上不做 PDF 重排。</p>
      <div class="drop"><span class="big">⬆</span>点击或拖入书（可多选 · 仅 EPUB / PDF）</div><input type="file" multiple hidden accept=".epub,.pdf">
      <ul class="q"></ul>
      <div class="row"><button class="btn pri go">开始上传</button></div>
    </div>
    <div class="card"><div id="bstat" class="kv small"></div>
      <h3>未完成 / 失败</h3><ul class="list" id="inbox"></ul></div>
  </div>
  <div class="subpanel">
    <div class="card"><h3 style="margin-top:0">字体（原生阅读器）</h3>
      <p class="small">ttf / otf → 装进 fontconfig 用户字体目录。上传后阅读器「文字与布局」菜单重开即可选，无需重启。KOReader 的字体在 KOReader 标签页装。</p>
      <div id="fbchain" class="opt-note" style="display:none"></div>
      <div class="row"><label class="toggle"><input type="checkbox" id="embold" checked> 中文加粗（墨水屏细笔画补偿）</label> <span class="small">默认开：对回退中文字体加粗，宋体在低对比墨水屏发淡时更清楚；翻书即见。</span></div>
      <div class="drop"><span class="big">🔤</span>点击或拖入 ttf/otf（可多选）</div><input type="file" multiple hidden accept=".ttf,.otf,.ttc">
      <ul class="q"></ul><div class="row"><button class="btn pri go">上传字体</button></div>
      <h3>已装字体</h3><ul class="list" id="fontlist"></ul></div>
  </div>`;
  let annotFolder='';   // 由 status 填充；「批注」预设投这里（合并后统一走 native，只是落地文件夹不同）
  const folderVal=()=>{const p=$('#folderPreset',sec).value;return p==='lib'?'':p==='annot'?annotFolder:$('#folder',sec).value.trim()};
  const syncFolder=()=>{$('#folder',sec).style.display=$('#folderPreset',sec).value==='custom'?'':'none'};
  // localStorage 记住上次选择
  $('#folderPreset',sec).value=LS.get('fpreset','lib');$('#opt',sec).value=LS.get('opt','auto');$('#chk',sec).checked=LS.get('chk','1')!=='0';$('#folder',sec).value=LS.get('folder','');
  $('#folderPreset',sec).onchange=()=>{syncFolder();LS.set('fpreset',$('#folderPreset',sec).value)};
  $('#opt',sec).onchange=()=>LS.set('opt',$('#opt',sec).value);
  $('#chk',sec).onchange=()=>LS.set('chk',$('#chk',sec).checked?'1':'0');
  $('#folder',sec).oninput=()=>LS.set('folder',$('#folder',sec).value);
  syncFolder();
  // 两个上传器：书(drop 0)、字体(drop 1)——按 DOM 顺序取；第 4 参 = 客户端预拦的允许扩展名
  const drops=sec.querySelectorAll('.drop'),inputs=sec.querySelectorAll('input[type=file]'),qs=sec.querySelectorAll('ul.q'),gos=sec.querySelectorAll('.go');
  const wrap=(k)=>({querySelector:(x)=>({'ul.q':qs[k],'input[type=file]':inputs[k],'.drop':drops[k],'.go':gos[k]}[x])});
  uploader(wrap(0),()=>'/api/books',()=>({folder:folderVal(),target:'native',optimize:$('#opt',sec).value,check:$('#chk',sec).checked?'on':'off'}),['.epub','.pdf']);
  uploader(wrap(1),()=>'/api/fonts',()=>({}),['.ttf','.otf','.ttc']);
  const refresh=async()=>{const s=await j('/api/books/status');if(s.ok&&s.annotFolder)annotFolder=s.annotFolder;$('#bstat',sec).innerHTML=s.ok?`<b>xochitl 投递</b><span>${s.uploadReachable?'✅ 可达':'<span style="color:var(--bad)">⚠ 不可达（lo 别名 / USB 未就绪）</span>'}</span><b>书库 / 批注</b><span>${s.libraryFolder} / ${s.annotFolder}</span><b>队列</b><span>待处理 ${s.spool.pending} · 失败 ${s.spool.failed}</span>${s.readingQol?`<b>阅读增强</b><span>点击翻页 ${s.readingQol.tapPageTurn?'开':'关'} · 快速黑白 ${s.readingQol.fastMono?'开':'关'} · 清残影 ${s.readingQol.refresh?'开':'关'} · 字体增强 ${s.readingQol.fontEnhance?'开':'关'}<br><span class="small">在设备「设置 → 系统增强」里改</span></span>`:''}`:`<b>book-serve</b><span>${s.message}</span>`;
    const ib=await j('/api/books/inbox');const ul=$('#inbox',sec);ul.innerHTML='';(ib.items||[]).forEach(it=>{const li=document.createElement('li');li.style.flexWrap='wrap';li.innerHTML=`<span>${it.name} <span class="small">${it.state} · ${fmtB(it.bytes)}</span></span><span>${it.state==='failed'?'<button class="btn r">重试</button> <button class="btn d">删除</button>':''}</span>${it.reason?`<div class="small" style="flex-basis:100%;color:var(--bad)">${it.reason}</div>`:''}`;
      if(it.state==='failed'){$('.r',li).onclick=async()=>{await j('/api/books/inbox/retry',{method:'POST',body:JSON.stringify({name:it.name})});refresh()};$('.d',li).onclick=async()=>{await j('/api/books/inbox/delete',{method:'POST',body:JSON.stringify({name:it.name})});refresh()}}
      ul.appendChild(li)});if(!(ib.items||[]).length)ul.innerHTML='<li class="small">（空）</li>';
    const fl=await j('/api/fonts');const fu=$('#fontlist',sec);fu.innerHTML='';
    // 中文缺字回退链（B1）：覆盖率≥8% 的中文字体，按覆盖率降序
    const cjk=(fl.items||[]).filter(it=>((it.extra||{}).cjkPct||0)>=8).sort((a,b)=>(b.extra.cjkPct||0)-(a.extra.cjkPct||0));
    const fb=$('#fbchain',sec);if(fb){fb.style.display='';fb.innerHTML=cjk.length?`中文缺字回退：${cjk.map(it=>`${it.name} <span class="small">${it.extra.cjkPct}%</span>`).join(' → ')}`:'⚠ 未装中文字体，正文缺字会显示方框——传一个全覆盖中文字体即可兜底。'}
    const fst=await j('/api/fonts/status');const eb=$('#embold',sec);if(eb&&fst.ok){eb.checked=!!fst.emboldenCjkFallback;eb.onchange=async()=>{const r=await j('/api/fonts/config',{method:'PUT',body:JSON.stringify({emboldenCjkFallback:eb.checked})});if(r.ok===false){alert(r.message);eb.checked=!eb.checked}}}
    (fl.items||[]).forEach(it=>{const ex=it.extra||{};const li=document.createElement('li');
      const left=document.createElement('span');left.innerHTML=`${it.name}${ex.names&&ex.names.cn&&ex.names.cn!==it.name?' <span class="small">'+ex.names.cn+'</span>':''}${ex.files&&ex.files.length>1?' <span class="small">×'+ex.files.length+'</span>':''}`;
      const right=document.createElement('span');right.style.cssText='display:flex;align-items:center;gap:.4em';right.className='small';
      const p=ex.cjkPct;if(p!=null){const cls=p>=80?'on':(p>=8?'':'off');right.insertAdjacentHTML('beforeend',`<span class="badge ${cls}" title="中文基本区覆盖率">中文 ${p}%</span>`)}
      if(ex.fontconfigRef)right.insertAdjacentHTML('beforeend','<span title="界面中文回退引用">⚠</span>');
      const d=document.createElement('button');d.className='btn';d.textContent='删除';d.onclick=async()=>{if(confirm('删除字体 '+it.name+'？')){const r=await j('/api/fonts/'+encodeURIComponent(it.name),{method:'DELETE'});if(r.ok===false)alert(r.message);refresh()}};right.appendChild(d);
      li.append(left,right);fu.appendChild(li)});if(!(fl.items||[]).length)fu.innerHTML='<li class="small">（空）</li>'};
  refresh();sec.refresh=refresh;subtabs(sec);}},
 'koreader-serve':{title:'KOReader',render(sec){sec.innerHTML=`
  <div class="subnav"><button class="on">📚 书库</button><button>🔤 字体</button><button>📖 词典</button></div>
  <div class="subpanel on">
    <div class="card"><h2>KOReader</h2><div class="kv small" id="ks" style="margin-top:.5em">加载…</div>
      <details class="cmp"><summary>KOReader 怎么装 / 已经帮你调好了什么</summary>
      <dl class="help">
        <dt>安装</dt><dd>走官方仓库自装：先备齐基石 xovi + appload（见「管理」页），再从 <a href="https://github.com/koreader/koreader/releases" target="_blank" rel="noopener">官方 releases</a> 装 reMarkable Paper Pro（rmpp）版。装好后这里就能看到藏书、传字体 / 词典。</dd>
        <dt>已按 Move 屏调好（开箱即用，不用手动配）</dt><dd>本套件的 profile 贴近 xochitl 观感：中文主字体霞鹜新致宋、页边距、行距、脚注<b>底部弹窗</b>、悬挂标点、防误触、退出手势。</dd>
        <dt>传什么</dt><dd>书任意格式、字节不改；字体 ttf/otf 只进 KOReader（原生阅读器的字体在 xochitl 页装）；词典填名后把一套 StarDict 文件（.ifo/.idx/.dict…）一起拖进来。</dd>
        <dt>改配置 / 删字体后</dt><dd>KOReader 若正在跑，需<b>重启它</b>才生效（下方状态「运行中」会提示）。</dd>
      </dl></details></div>
    <div class="card"><h3 style="margin-top:0">书库 <span id="kcrumb" class="small crumb"></span></h3>
    <div class="drop"><span class="big">⬆</span>拖入书到当前目录（可多选 · 任意格式原样）</div><input type="file" multiple hidden><ul class="q"></ul><div class="row"><button class="btn pri go">上传到当前目录</button></div>
    <ul class="list" id="kb"></ul></div>
  </div>
  <div class="subpanel">
    <div class="card"><h3 style="margin-top:0">字体（KOReader）</h3><p class="small">只装进 KOReader；原生阅读器的字体在 xochitl 标签页装。</p>
    <div class="drop"><span class="big">🔤</span>点击或拖入 ttf/otf（可多选）</div><input type="file" multiple hidden accept=".ttf,.otf,.ttc"><ul class="q"></ul><div class="row"><button class="btn pri go">上传字体</button></div>
    <h3>已装字体</h3><ul class="list" id="kf"></ul></div>
  </div>
  <div class="subpanel">
    <div class="card"><h3 style="margin-top:0">词典（KOReader）</h3><p class="small">StarDict 词典：填词典名，拖入这本词典的全部文件（.ifo/.idx/.dict/.dz/.syn/.oft）一起传。</p>
    <label class="field" for="dictname">词典名</label><input type="text" id="dictname" placeholder="如 牛津高阶 / cc-cedict">
    <div class="drop"><span class="big">📖</span>点击或拖入词典文件（可多选）</div><input type="file" multiple hidden accept=".ifo,.idx,.dict,.dz,.syn,.oft"><ul class="q"></ul><div class="row"><button class="btn pri go">上传词典</button></div>
    <h3>已装词典</h3><ul class="list" id="kd"></ul></div>
  </div>`;
  let kdir='';
  const drops=sec.querySelectorAll('.drop'),inputs=sec.querySelectorAll('input[type=file]'),qs=sec.querySelectorAll('ul.q'),gos=sec.querySelectorAll('.go');
  const wrap=(i)=>({querySelector:(sel)=>({'ul.q':qs[i],'input[type=file]':inputs[i],'.drop':drops[i],'.go':gos[i]}[sel])});
  uploader(wrap(0),()=>'/api/koreader/books',()=>({folder:kdir}));   // 任意格式原样，不预拦
  uploader(wrap(1),()=>'/api/koreader/fonts',()=>({}),['.ttf','.otf','.ttc']);
  uploader(wrap(2),()=>'/api/koreader/dicts',()=>({name:$('#dictname',sec).value.trim()}),['.ifo','.idx','.dict','.dz','.syn','.oft']);
  const refresh=async()=>{const s=await j('/api/koreader/status');$('#ks',sec).innerHTML=s.ok?`<b>安装</b><span>${s.installed?'是':'否'} ${s.version?'('+s.version+')':''}</span><b>运行中</b><span>${s.running?'是（改配置 / 删字体后需重启它）':'否'}</span><b>目录</b><span>${s.root}</span><b>藏书</b><span>${s.books} 本 · 字体 ${s.fonts} 个 · 词典 ${s.dicts||0} 本</span>`:`<span>${s.message}</span>`;
    const f=await j('/api/koreader/fonts');const uf=$('#kf',sec);uf.innerHTML='';(f.items||[]).forEach(it=>{const li=document.createElement('li');
      const badge=it.cjkPct!=null?`<span class="badge ${it.cjkPct>=80?'on':(it.cjkPct>=8?'':'off')}" title="中文基本区覆盖率">中文 ${it.cjkPct}%</span> `:'';
      li.innerHTML=`<span>${it.name}</span><span class="small">${badge}${fmtB(it.bytes)} </span>`;const d=document.createElement('button');d.className='btn';d.textContent='删除';d.onclick=async()=>{if(confirm('从 KOReader 删除 '+it.name+'？')){const r=await j('/api/koreader/fonts/'+encodeURIComponent(it.name),{method:'DELETE'});if(r.ok===false)alert(r.message);refresh()}};li.lastChild.appendChild(d);uf.appendChild(li)});if(!(f.items||[]).length)uf.innerHTML='<li class="small">（空）</li>';
    const dc=await j('/api/koreader/dicts');const ud=$('#kd',sec);ud.innerHTML='';(dc.items||[]).forEach(it=>{const li=document.createElement('li');li.innerHTML=`<span>📖 ${it.name}</span><span class="small">${it.ifo} 本</span>`;ud.appendChild(li)});if(!(dc.items||[]).length)ud.innerHTML='<li class="small">（空）</li>';
    const b=await j('/api/koreader/books?'+new URLSearchParams({folder:kdir}));const ul=$('#kb',sec);ul.innerHTML='';
    const crumb=$('#kcrumb',sec);crumb.innerHTML='';const parts=kdir?kdir.split('/'):[];const mk=(t,p)=>{const a=document.createElement('a');a.href='#';a.textContent=t;a.onclick=e=>{e.preventDefault();kdir=p;refresh()};return a};crumb.appendChild(mk('根',''));parts.forEach((p,i)=>{crumb.append(' / ');crumb.appendChild(mk(p,parts.slice(0,i+1).join('/')))});
    (b.items||[]).forEach(it=>{const li=document.createElement('li');if(it.kind==='dir'){li.innerHTML=`<span>📁 <a href="#">${it.name}</a></span><span class="small">${it.count} 本</span>`;$('a',li).onclick=e=>{e.preventDefault();kdir=(kdir?kdir+'/':'')+it.name;refresh()}}else li.innerHTML=`<span>${it.name}</span><span class="small">${fmtB(it.bytes)}</span>`;ul.appendChild(li)});if(!(b.items||[]).length)ul.innerHTML='<li class="small">（空目录）</li>'};
  refresh();sec.refresh=refresh;subtabs(sec)}},
 'wallpaper-serve':{title:'壁纸',render(sec){assetTab(sec,'/api/wallpapers','jpg / png 图片，自动裁到 954×1696。首张自动启用，下次休眠即生效。',{icon:'🖼',accept:['.jpg','.jpeg','.png'],
   header:`<label class="field">休眠轮换</label><div class="row"><select id="wpmode" style="max-width:12em"><option value="sequential">按顺序</option><option value="random">随机</option><option value="fixed">固定</option></select><span id="wpst" class="small"></span></div>`,
   onRender:async(sec,refresh)=>{const st=await j('/api/wallpapers/status');const sel=$('#wpmode',sec);if(st.ok){sel.value=st.mode;LS.set('wpmode',st.mode);$('#wpst',sec).textContent=`当前 ${st.current||'（无）'} · 已挂载 ${st.mounted}/${st.expectedMounts}`}
     sel.onchange=async()=>{LS.set('wpmode',sel.value);await j('/api/wallpapers/mode',{method:'PUT',body:JSON.stringify({mode:sel.value})});refresh()}},
   itemAction:(it,refresh)=>{if((it.extra||{}).current)return '<span class="badge on">当前</span>';const b=document.createElement('button');b.className='btn';b.textContent='使用';b.onclick=async()=>{await j('/api/wallpapers/current',{method:'PUT',body:JSON.stringify({name:it.name})});refresh()};return b},
   preview:it=>`<img src="/api/wallpapers/${encodeURIComponent(it.name)}" alt="" style="height:3.4em;border-radius:.3em;border:1px solid var(--line);margin-right:.6em;vertical-align:middle">`})}},
 'weread-serve':{title:'微读',render(sec){sec.innerHTML='<div class="card"><h2>微信读书</h2><p class="lead">网页版入口尚未上线（rmweb × Move 门控 spike 中）。通过前这里不提供功能。</p></div>'}}
};

function assetTab(sec,api,hint,ext={}){sec.innerHTML=`<div class="card"><p class="lead">${hint}</p>${ext.header||''}
  <div class="drop"><span class="big">${ext.icon||'⬆'}</span>点击或拖入文件（可多选）</div><input type="file" multiple hidden${ext.accept?' accept="'+ext.accept.join(',')+'"':''}><ul class="q"></ul><div class="row"><button class="btn pri go">上传</button></div>
  <h3>已安装</h3><ul class="list" id="al"></ul></div>`;
  uploader(sec,()=>api,()=>({}),ext.accept);
  const refresh=async()=>{const d=await j(api);const ul=$('#al',sec);ul.innerHTML='';(d.items||[]).forEach(it=>{const ex=it.extra||{};const li=document.createElement('li');
    const left=document.createElement('span');left.innerHTML=(ext.preview?ext.preview(it):'')+`${it.name}${ex.names&&ex.names.cn&&ex.names.cn!==it.name?' <span class="small">'+ex.names.cn+'</span>':''}${ex.files&&ex.files.length>1?' <span class="small">×'+ex.files.length+'</span>':''}`;
    const right=document.createElement('span');right.style.display='flex';right.style.alignItems='center';right.style.gap='.4em';right.className='small';
    if(it.bytes)right.insertAdjacentHTML('beforeend','<span>'+fmtB(it.bytes)+'</span>');
    if(ext.itemAction){const a=ext.itemAction(it,refresh);if(typeof a==='string'){if(a)right.insertAdjacentHTML('beforeend',a)}else right.appendChild(a)}
    if(ex.fontconfigRef)right.insertAdjacentHTML('afterbegin','<span title="被界面中文回退引用，删了界面可能变方块">⚠</span>');
    if(!ex.current){const del=document.createElement('button');del.className='btn';del.textContent='删除';del.onclick=async()=>{const files=(ex.files||[]).length>1?'（含 '+ex.files.length+' 个文件）':'';if(confirm('删除 '+it.name+files+(ex.fontconfigRef?'？\n⚠ 该字体是界面中文回退字体':'？'))){const r=await j(api+'/'+encodeURIComponent(it.name),{method:'DELETE'});if(r.ok===false)alert(r.message);refresh()}};right.appendChild(del)}
    li.append(left,right);ul.appendChild(li)});if(!ul.children.length)ul.innerHTML='<li class="small">（空）</li>';if(ext.onRender)ext.onRender(sec,refresh)};
  refresh();sec.refresh=refresh}

/* 管理台/引导（固定 tab，始终在——它是网关自身页面，不由服务注册表驱动） */
function renderManage(sec){sec.innerHTML=`
  <div class="card"><h2>引导 · 基石</h2><p class="lead">书架的功能建在 xovi + appload 之上。先用桌面端 <b>reManager</b>（或设备上的 vellum）把基石装好，KOReader 走官方仓库自装，再回这里管理书架各功能。</p>
    <div class="kv small" id="found">检测中…</div>
    <p class="small">下载 / 文档：<a href="https://github.com/rmitchellscott/reManager" target="_blank" rel="noopener">reManager</a>（桌面端 · vellum 生态）· <a href="https://github.com/asivery/rmpp-xovi" target="_blank" rel="noopener">xovi</a> · <a href="https://github.com/koreader/koreader/wiki" target="_blank" rel="noopener">KOReader Wiki</a></p></div>
  <div class="card"><h2>电脑端 · <code>shelf push</code>（进阶洗书 / PDF 重排）</h2>
    <p class="lead">网页直传已够日常。电脑端多一条 Calibre 高质量路，专治难搞的书——最强但容易被忽略。</p>
    <details class="cmp"><summary>它比网页直传强在哪 / 怎么装 / 怎么用</summary>
    <dl class="help">
      <dt>强在哪</dt>
      <dd>① <b>杂格式转干净 EPUB</b>：AZW3 / MOBI / FB2 / TXT… 网页端一律拒收，这里能转；<br>
          ② <b>Calibre 级深洗</b>：CSS 拍平比端上更彻底，排版锁得死的书也能救；<br>
          ③ <b>PDF 学术重排</b>：多列论文 / 公式 / 图按阅读顺序重排到设备屏宽——<b>端上做不到</b>（端上 PDF 只能原样直传）；<br>
          ④ 扫描件走 k2pdfopt 位图重排。产物再叠加设备同款优化器，最终与网页直传的书观感一致。</dd>
      <dt>怎么装</dt>
      <dd>电脑装 <a href="https://calibre-ebook.com/" target="_blank" rel="noopener">Calibre</a>（含 ebook-convert）+ Python 3；克隆本仓库，<code>cd shelf &amp;&amp; sh build.sh</code> 编出 <code>epub-optimize</code>；投书用 <code>shelf/host/bin/shelf</code>。</dd>
      <dt>常用命令</dt>
      <dd><code>shelf push 书.epub</code> 洗书投书库<br>
          <code>shelf push 论文.pdf</code> PDF 重排后投<br>
          <code>shelf push 书.azw3 -t koreader</code> 投 KOReader<br>
          <code>shelf push 书.epub -f 论文/2026</code> 投指定文件夹<br>
          <code>shelf status</code> / <code>shelf doctor</code> 看连通与环境</dd>
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
  svcs.forEach((s,i)=>addTab(TABS[s.name].title,TABS[s.name].render,i===0));
  addTab('管理',renderManage,!svcs.length);   // 固定管理台，始终可进（即使没领域服务在线）
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
