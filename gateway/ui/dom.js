const $=(s,r=document)=>r.querySelector(s);
// 小徽章：renderManage 的「基石与模块」列表用。
const badge=(t,ok)=>`<span class="badge ${ok?'on':'off'}">${t}</span>`;
// 「已加载/未加载」徽章（xovi 有没有进 xochitl）：管理页基石、设备健康的概览与扩展三处共用。
const xoviBadge=on=>badge(on?T('manage.loaded.on'):T('manage.loaded.off'),!!on);
/* 防双击：按钮点击后立即禁用，异步操作完成（不管成功失败）再解禁。很多按钮的异步操作是删除/
   落库这类不该被同一次操作重复触发两遍的动作——不加这一层，手指点快了或者网络慢的时候网络请求
   还没回来就能再点一次，2026-09-18 真机反馈过"处理中又点了删除"这类并发操作撞在一起。新按钮一律
   用下面的 btn()（内部就是这一层）。一开始就 disabled=true 的按钮（"根本点不了"，不是"点了在跑"）不传处理函数。*/
const guardClick=(el,fn)=>{el.onclick=async()=>{if(el.disabled)return;el.disabled=true;try{await fn()}catch(e){console.error(e);toast(T('common.failed'))}finally{el.disabled=false}}};
/* 按钮：`btn(文案, 点击处理, 类名)`——el('button')+guardClick 这一对全站出现二十来次，收成一处。点击处理可同步可异步
   （guardClick 统一防双击、异常提示）；文案可以是字符串或节点数组（底部栏的"标签+数量角标"）；extra 放 title 等其余属性。 */
const btn=(text,fn,cls='btn',extra)=>{const b=el('button',{class:cls,type:'button',...extra},typeof text==='string'?null:text);if(typeof text==='string')b.textContent=text;if(fn)guardClick(b,fn);return b};
/* 轻量 DOM 构建 helper：`el('div',{class:'small',style:'...'},[child1,child2])`。`attrs` 里
   `class`/其余属性走 `setAttribute`，`style` 走 `style.cssText`，`text`/`html` 分别设
   `textContent`/`innerHTML`；`children` 接单个节点/字符串或数组。不强求把全站模板字符串都改成它——
   只在同类节点密集、或要插外部数据的地方用（`text` 天然不需要转义）。*/
const el=(tag,attrs,children)=>{const n=document.createElement(tag);
  if(attrs)for(const k in attrs){const v=attrs[k];if(k==='style')n.style.cssText=v;else if(k==='text')n.textContent=v;else if(k==='html')n.innerHTML=v;else n.setAttribute(k,v)}
  if(children!=null)for(const c of [].concat(children))n.appendChild(typeof c==='string'?document.createTextNode(c):c);
  return n};
/* 忙态进度：不确定态滚动条（浏览器原生 `<progress>` 不带 value/max 渲染成不确定态动画）+ 一行说明——母版库行内、
   笔记「推送本章」共用（`container` 是要挂这块的父节点，自己占一整行）。原来还能按 `{done,total}` 画真百分比，那是给设备上
   「优化」和分卷投递用的，2026-10-07 起没有调用方传分步数据，删掉。 */
const renderBusy=(container,label)=>{
  const wrap=el('div',{style:'flex-basis:100%;margin-top:.2em'},[el('progress'),el('div',{class:'small',text:label})]);
  container.appendChild(wrap);
  return wrap};
/* 全局 toast：系统里不允许用浏览器原生 alert（打断操作、要点掉才能继续，风格跟页面其它地方的行内
   小字状态提示完全不一致），全站原来散落的 26 处 alert() 统一改走这个（2026-09-19 用户明确要求）。
   #toasthost 惰性建：第一次调用 toast() 时才挂进 body，不用改 index.html。kind 决定配色（跟徽章
   同一套 --ok/--bad/--warn 变量，见 style.css），缺省 'bad'——历史上这堆 alert() 十有八九是报错。
   点一下提前关掉；到时自动淡出+移除。 */
const toastHost=()=>{let el=document.getElementById('toasthost');if(!el){el=document.createElement('div');el.id='toasthost';document.body.appendChild(el)}return el};
const showToast=(children,kind,ms)=>{const t=el('div',{class:'toast '+kind},children);toastHost().appendChild(t);
  requestAnimationFrame(()=>t.classList.add('show'));
  const kill=()=>{t.classList.remove('show');setTimeout(()=>t.remove(),200)};
  t.onclick=kill;setTimeout(kill,ms)};
const toast=(msg,kind='bad',ms=4200)=>{if(msg)showToast(msg,kind,ms)};
/* 带一个链接的提示（停留更久）：点链接是用户手势，打开新页不会被浏览器当弹窗拦掉。 */
const toastLink=(msg,linkText,href,ms=15000)=>showToast([msg+' ',el('a',{href,target:'_blank',rel:'noopener',text:linkText})],'ok',ms);
/* 两种对话框（确认 / 输入）的公共骨架：遮罩 + 盒子 + 关闭收尾（摘掉键盘监听、移除节点、兑现 Promise）。
   点遮罩 / Esc = 取消（cancelValue）；onKey 处理其余按键（Enter 等）。build(close) 返回盒子里的节点数组与要聚焦的元素。 */
const modal=(cancelValue,build,onKey)=>new Promise(resolve=>{
  const close=v=>{document.removeEventListener('keydown',key);overlay.remove();resolve(v)};
  const {nodes,focus}=build(close);
  const overlay=el('div',{class:'confirm-overlay'},[el('div',{class:'confirm-box'},nodes)]);
  // 焦点在对话框里的某个按钮上时，Enter 交给那个按钮自己（浏览器把它当点击）：此前 Tab 到「否」再按 Enter，
  // 文档级的 Enter=确认 先触发，删除之类的操作就被当成"是"执行了。
  const key=e=>{if(e.key==='Escape')close(cancelValue);else if(e.key==='Enter'&&e.target.tagName==='BUTTON')return;else if(onKey)onKey(e,close)};
  overlay.onclick=e=>{if(e.target===overlay)close(cancelValue)};
  document.addEventListener('keydown',key);
  document.body.appendChild(overlay);
  if(focus)focus();
});
/* 自定义确认框：浏览器原生 confirm() 跟已经禁掉的 alert() 是同一类问题——阻塞整个页面、样式跟
   站内其它地方完全脱节，全站原来散落的 9 处 confirm() 统一改走这个（2026-09-19 用户反馈"母版库
   删除确认还是 alert"——严格说原来用的是 confirm() 不是 alert()，但对用户来说是同一类"浏览器
   弹出个原生对话框"体验，touch 一次改到底，不分是 alert 还是 confirm）。返回 `Promise<boolean>`，
   调用方需要 `await`（跟原来 `if(confirm(msg))` 同步调用不一样，全部改成
   `if(await confirmDialog(msg))`）；点"是"/`Enter`/取消按钮外没有对应处理，点遮罩/`Esc`/"否"
   都算取消，跟原生 confirm() 的"确定/取消"行为对齐。 */
const confirmDialog=msg=>modal(false,close=>{
  const yesBtn=btn(T('common.yes'),()=>close(true),'btn pri'),noBtn=btn(T('common.no'),()=>close(false));
  return {nodes:[el('div',{class:'confirm-msg',text:msg}),el('div',{class:'confirm-actions'},[noBtn,yesBtn])],focus:()=>yesBtn.focus()};
},(e,close)=>{if(e.key==='Enter')close(true)});
/* 输入框版的 confirmDialog：返回 `Promise<string|null>`（取消 = null）。给改名这类"要用户敲一个值"的操作用。 */
const promptDialog=(msg,value='')=>{const inp=el('input',{type:'text',class:'confirm-input'});inp.value=value;
  return modal(null,close=>{
    const yesBtn=btn(T('common.ok'),()=>close(inp.value),'btn pri'),noBtn=btn(T('common.cancel'),()=>close(null));
    return {nodes:[el('div',{class:'confirm-msg',text:msg}),inp,el('div',{class:'confirm-actions'},[noBtn,yesBtn])],focus:()=>{inp.focus();inp.select()}};
  },(e,close)=>{if(e.key==='Enter')close(inp.value)})};
/* 开关复选框绑定 PUT：勾选即 PUT `{key:checked}`，期间禁用；失败弹 toast 并把勾选还原。「系统增强」/「实验室」/字体加粗/合书自动转写共用。 */
const bindToggle=(box,url,key)=>{box.onchange=async()=>{const want=box.checked;box.disabled=true;
  const r=await sendT(url,'PUT',{[key]:want},'common.saveFailed');box.disabled=false;if(r.ok===false)box.checked=!want}};

/* 上传区 HTML（拖放框 + 隐藏 input + 队列 + 按钮），一处生成、各页复用；uploader() 认这个 .up 容器 */
const upHtml=(icon,label,ext,btn)=>`<div class="up"><div class="drop"><span class="big">${icon}</span>${label}</div><input type="file" multiple hidden accept="${ext.join(',')}"><ul class="q"></ul><div class="row"><button class="btn pri go">${btn}</button></div></div>`;

/* 通用上传器：逐文件一请求，进度条，逐项回执；失败项可重传，队列可逐项删/清空，顶部总进度。box=.up 容器，url=上传接口。
   （原来 url 与查询参数各是一个回调，四个调用点全传常量 url + 空查询，2026-10-09 收成一个字符串。） */
function uploader(box,url,okExt,onFinish,dedupeApi){
  const list=$('ul.q',box), input=$('input[type=file]',box), drop=$('.drop',box), go=$('.go',box);
  let files=[], sum=null, busy=false;
  const clr=btn(T('common.clear'),()=>{files=[];render()});go.after(clr);
  const summary=()=>{if(!sum){sum=el('div',{class:'small',style:'margin:.3em 0'});list.parentNode.insertBefore(sum,list)}
    const ok=files.filter(f=>f.st==='ok').length,bad=files.filter(f=>f.st==='bad').length;
    sum.innerHTML=files.length?T('common.uploadSummary',{ok,total:files.length,badPart:bad?T('common.uploadBadPart',{bad}):''}):'';};
  const row=f=>{const li=document.createElement('li');li.dataset.k=f.k;li.className=f.st||'';
      li.innerHTML=`<div class="name">${esc(f.file.name)} <span class="small">${fmtB(f.file.size)}</span> <button class="btn x" type="button" title="${T('common.remove')}" aria-label="${T('common.remove')}">×</button></div><progress value="${f.st==='ok'?100:0}" max="100"></progress><div class="msg">${esc(f.msg||T('common.waitingUpload'))}</div>`;
      const x=li.querySelector('.x');x.disabled=busy;x.onclick=()=>{if(busy)return;files=files.filter(y=>y.k!==f.k);render()};return li};
  const render=()=>{list.innerHTML='';files.forEach(f=>list.appendChild(row(f)));summary()};
  /* 上传进行中不整表重画（删行按钮禁用、新加的文件只追加行）：重画会把正在传的那一项的进度条/状态文字换成新节点，
     上传回调还挂在旧节点上，之后再也不更新。新追加的文件本轮循环会接着传（循环遍历的就是 files 这个数组）。 */
  const add=fl=>{for(const f of fl){const rej=okExt&&!okExt.some(e=>f.name.toLowerCase().endsWith(e));
      const it={file:f,k:Math.random().toString(36).slice(2),rej,st:rej?'bad':'',msg:rej?T('common.rejectedExt',{ext:okExt.join(' / ')}):''};
      files.push(it);if(busy)list.appendChild(row(it))}
    if(busy)summary();else render()};
  input.onchange=()=>{add(input.files);input.value=''};
  drop.ondragover=e=>{e.preventDefault();drop.classList.add('hi')};drop.ondragleave=()=>drop.classList.remove('hi');
  drop.ondrop=e=>{e.preventDefault();drop.classList.remove('hi');add(e.dataTransfer.files)};
  drop.onclick=()=>input.click();
  const lockRows=on=>{busy=on;list.querySelectorAll('.x').forEach(x=>x.disabled=on)};
  go.onclick=async()=>{go.disabled=true;clr.disabled=true;lockRows(true);
    // 母版库上传口传 dedupeApi（`/api/books/staging`）：先查一次现有条目，同名同大小＝上一轮已经成功落地，跳过重传，
    // 省流量省时间（主要是大 PDF）。只是省传输：服务端入库本来就逐字节比对，内容相同直接认已有那本、不会落出第二份；
    // EPUB 入库时会规范化文件名，按原名查常常对不上，这时照常上传、由服务端去重。只有母版库这个上传口传这个参数，
    // 字体/壁纸那几个 uploader() 调用点不传。查询失败（网络/未登录）就当没查到，照常全部传。
    let existing=null;
    if(dedupeApi){try{const d=await j(dedupeApi);existing=new Set((d.items||[]).map(it=>it.name+'|'+it.bytes))}catch{existing=null}}
    for(const f of files){if(f.st==='ok'||f.rej)continue;          // 成功项跳过；格式不收项不上传；失败项允许重传
      const li=list.querySelector(`li[data-k="${f.k}"]`);if(!li)continue;const pg=$('progress',li),msg=$('.msg',li);
      if(existing&&existing.has(f.file.name+'|'+f.file.size)){f.st='ok';li.className='ok';pg.value=100;f.msg=T('common.alreadyStaged');msg.textContent=f.msg;summary();continue}
      f.st='';li.className='';pg.value=0;msg.textContent=T('common.uploading');
      await new Promise(res=>{const x=new XMLHttpRequest();x.open('POST',url);
        x.upload.onprogress=e=>{if(e.lengthComputable)pg.value=e.loaded/e.total*100};
        // 401/403 与 j() 同一处理（登录过期 → 登录页；首登未改密 → 改密页）；非 JSON 应答给人话 + 状态码（原来是裸 "HTTP 502"，不走语言包）。
        x.onload=()=>{if(authRedirect(x.status))return;
          let d;try{d=JSON.parse(x.responseText)}catch{d={ok:false,message:T('common.httpErr',{status:x.status})}}
          const it=(d.items&&d.items[0])||d;f.st=it.ok?'ok':'bad';f.msg=(it.message||(it.ok?T('common.done'):T('common.failed')))+(d.note&&it.ok?' · '+d.note:'');li.className=f.st;msg.textContent=f.msg;pg.value=100;summary();
          // 同一批里排了两份同名同大小：这份传完了要马上补进快照，下一份循环到时才躲得开——只查一次
          // 快照、循环里不更新的话，两份会一起溜过去（都不在最初那份快照里），2026-09-13 真机踩到。
          if(existing&&it.ok)existing.add(f.file.name+'|'+f.file.size);
          res()};
        x.onerror=()=>{f.st='bad';f.msg=T('common.networkError');li.className='bad';msg.textContent=f.msg;summary();res()};
        const fd=new FormData();fd.append('file',f.file);x.send(fd)})}
    lockRows(false);go.disabled=false;clr.disabled=false;if(onFinish)onFinish()};
  return {clear(){files=[];render()}};
}

/* 二级标签：面板都由外层 render/refresh 预先填好，切换只显隐。`:scope >` 限定只找 sec 的**直接
   子元素**：subnav 会嵌套（「管理 → 设备健康」有自己的一层，笔记页也有；2026-09-30 前还有「管理 → 电池刺客 → 耗电情况 → 时间窗」），不加
   `:scope >` 的话外层 querySelectorAll('.subpanel') 会把内层的 subpanel 也扫进来，按钮与面板按下标配对就错位。 */
function subtabs(sec){const nav=sec.querySelector(':scope > .subnav');if(!nav)return;const btns=[...nav.children],panels=[...sec.querySelectorAll(':scope > .subpanel')];
  btns.forEach((b,i)=>b.onclick=()=>{btns.forEach(x=>x.classList.remove('on'));panels.forEach(p=>p.classList.remove('on'));b.classList.add('on');if(panels[i])panels[i].classList.add('on')});}

/* 列表渲染骨架：每项一行「左：名字等 ｜ 右：徽章/大小/按钮」；row(it,left,right,li) 填内容。字体/词典/壁纸共用 */
// emptyMsg 可选：不给就用通用的"（空）"，母版库/浏览页/回收站这几处早就有各自的引导式空状态文案，
// 这里字体/词典/壁纸列表原来共用的"（空）"完全没有引导，跟其它页面不一致（2026-09-09 审计发现）——
// 各调用点按自己的场景传一句"去哪里做什么"。
function fillList(ul,items,row,emptyMsg){ul.innerHTML='';if(!items.length){ul.innerHTML=`<li class="small">${emptyMsg||T('list.empty')}</li>`;return}
  items.forEach(it=>{const li=el('li'),left=el('span'),right=el('span',{class:'small',style:'display:flex;align-items:center;gap:.4em;flex-wrap:wrap'});
    row(it,left,right,li);li.append(left,right);ul.appendChild(li)})}
/* 删除按钮：confirmDialog → DELETE → 刷新 */
const delBtn=(msg,url,refresh)=>btn(T('action.delete'),async()=>{if(await confirmDialog(msg)){await sendT(url,'DELETE');refresh()}});
