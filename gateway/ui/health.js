/* 「管理 → 设备健康」（2026-09-25，gateway/src/device/）：只读体检 + 清理遗留数据。**只在这一屏被打开、或点刷新时
   取数**，不跟管理页其它子标签一起刷、不订阅任何定时器（设备要省电）。网关侧结果缓存 15 秒，刷新按钮带 fresh=1 现采。
   2026-09-25 同日用户反馈"太长"，拆成五个二级 tab（概览/服务/扩展/日志/清理，subtabs() 惯例，第三层嵌套靠它的
   `:scope >` 限定）：一次 /api/device/health（+ /api/device/ota）的结果分发到前四个 tab，切 tab 只显隐、不重取；
   清理是单独接口，进这一屏时它不在前台就只记"待取"，第一次切到它时才取。上次停在哪个二级 tab 记在 LS。 */
const fmtUptime=s=>{const d=Math.floor(s/86400),h=Math.floor(s%86400/3600),m=Math.floor(s%3600/60);
  return d?T('health.upDays',{d,h}):h?T('health.upHours',{h,m}):T('health.upMins',{m})};
// 两分钟以内按秒（开机时序要看到 0.01 s 级），更长的（服务开机后被重启过）换成"12 分钟"这类读法，不写 761.5 s。
const fmtSec=ms=>ms>=120000?fmtUptime(Math.floor(ms/1000)):(ms/1000).toFixed(ms<10000?2:1)+' s';
const fmtTime=secs=>secs?new Date(secs*1000).toLocaleString():'';
function mountHealth(box){
  const card=k=>`<div class="subpanel" data-p="${k}"><div class="card" data-body><p class="small">${T('health.loading')}</p></div></div>`;
  box.innerHTML=`<div class="card"><div class="row" style="justify-content:space-between;margin-top:0"><h2 style="margin:0">${T('health.title')}</h2><button class="btn" data-refresh>${T('health.refresh')}</button></div>
    <p class="lead">${T('health.lead')}</p><p class="small" data-at></p></div>
    <div class="subnav"><button>${T('health.tab.overview')}</button><button>${T('health.tab.services')}</button><button>${T('health.tab.extensions')}</button><button>${T('health.tab.log')}</button><button>${T('health.tab.cleanup')}</button></div>
    ${card('overview')}${card('services')}${card('extensions')}${card('log')}
    <div class="subpanel" data-p="cleanup"><div class="card" data-cleanup></div></div>`;
  const q=s=>box.querySelector(s),body=k=>q(`[data-p="${k}"] [data-body]`),refreshBtn=q('[data-refresh]');
  const cleanupLoad=mountCleanup(q('[data-cleanup]'));
  const CLEANUP=4;let cleanupStale=true;
  const cleanupIfShown=()=>{if(cleanupStale&&q('[data-p="cleanup"]').classList.contains('on')){cleanupStale=false;return cleanupLoad()}};
  subtabs(box);
  const btns=[...q('.subnav').children],saved=+LS.get('healthSub','0');
  // 恢复上次的二级 tab：先调 subtabs 装的原始切换，再包记忆/取数那层——挂载管理页时不该去取清理数据。
  btns[saved>=0&&saved<btns.length?saved:0].onclick();
  btns.forEach((b,i)=>{const sw=b.onclick;b.onclick=()=>{sw();LS.set('healthSub',String(i));if(i===CLEANUP)cleanupIfShown()}});
  const loadHealth=async fresh=>{
    const [d,o]=await Promise.all([j('/api/device/health'+(fresh?'?fresh=1':'')),j('/api/device/ota'+(fresh?'?fresh=1':''))]);
    if(d.ok===false){const m=`<p class="small">${esc(d.message)}</p>`;['overview','services','extensions','log'].forEach(k=>body(k).innerHTML=m);return}
    const units=d.units||[],xu=units.find(u=>u.unit==='xochitl.service')||{},x=d.xochitl||{};
    const none=`<span class="small">${T('health.none')}</span>`;
    const names=l=>l&&l.length?esc(l.join(' · ')):none;
    const fw=d.firmware||{};
    const fwTxt=fw.state==='done'?(fw.known?`<span class="badge on" title="${esc(fw.label||'')}">${esc(T('health.fw.known',{label:(fw.label||'').split(/\s/)[0]}))}</span>`
        :`<span class="badge off" title="${T('health.fw.unknownTitle')}">${T('health.fw.unknown')}</span>`)+` <code style="overflow-wrap:anywhere">${esc((fw.sha256||'').slice(0,16))}…</code>`
      :fw.state==='error'?`<span class="small">${esc(T('health.fw.error',{msg:fw.message||''}))}</span>`:`<span class="small">${T('health.fw.pending')}</span>`;
    // OTA 判定跟页头横幅同一个接口（device/ota.rs）；这里只是换个地方常驻显示，横幅被 × 掉之后也能在这看到。
    const otaTxt=o.ok===false?'<span class="small">—</span>':o.needsReinstall
      ?badge(o.recovery==='full'?T('ota.banner.title'):T('ota.banner.xoviTitle'),false)+`<br><span class="small">${(o.reasons||[]).map(r=>esc(T('ota.reason.'+r,{units:(o.missingUnits||[]).join(', ')}))).join('<br>')}</span>`
      :badge(T('health.ota.ok'),true);
    const home=d.home||{};
    const xoviTxt=x.readable?xoviBadge(x.xovi):'<span class="small">—</span>';
    q('[data-at]').textContent=T('health.at',{time:fmtTime(d.at)});
    body('overview').innerHTML=`<div class="kv small">
      <b>${T('health.uptime')}</b><span>${d.uptimeSecs!=null?fmtUptime(d.uptimeSecs):'—'}</span>
      <b>xochitl</b><span>${badge(esc(xu.active||'?'),xu.active==='active')} <span title="${T('health.restartsTitle')}">${T('health.unit.restarts',{n:xu.nRestarts??'?'})}</span>${xu.pid?' · PID '+xu.pid:''}</span>
      <b>${T('health.xovi')}</b><span>${xoviTxt}</span>
      <b>${T('health.home')}</b><span>${home.freeBytes!=null?T('health.homeVal',{free:fmtB(home.freeBytes),total:fmtB(home.totalBytes||0)}):'—'}</span>
      <b>${T('health.firmware')}</b><span>${fwTxt}</span>
      <b>${T('health.ota')}</b><span>${otaTxt}</span></div>`;
    body('services').innerHTML=`<h3 style="margin-top:0">${T('health.services')}</h3><p class="small">${T('health.servicesHint')}</p>`
      +(d.systemctlError?`<p class="small">${esc(T('health.systemctlError',{msg:d.systemctlError}))}</p>`:'')+'<ul class="list" data-units></ul>';
    const ul=body('services').querySelector('[data-units]');
    units.forEach(u=>{
      const missing=u.load==='not-found';
      const st=missing?`<span class="badge">${T('health.unit.notFound')}</span>`:badge(esc((u.active||'?')+(u.sub?' / '+u.sub:'')),u.active==='active');
      const bits=[];
      if(!missing&&u.nRestarts!=null)bits.push(`<span title="${T('health.restartsTitle')}">${T('health.unit.restarts',{n:u.nRestarts})}</span>`);
      if(u.rssKb!=null)bits.push(esc(T('health.unit.mem',{rss:fmtB(u.rssKb*1024),hwm:fmtB((u.hwmKb||0)*1024)})));
      if(u.startedAtMs!=null)bits.push(esc(u.startMs!=null&&u.startMs>=50?T('health.unit.started',{at:fmtSec(u.startedAtMs),dur:fmtSec(u.startMs)}):T('health.unit.startedAt',{at:fmtSec(u.startedAtMs)})));
      ul.appendChild(el('li',{class:'stack'},[el('span',{html:`${esc(u.unit.replace(/\.service$/,''))} ${st}`}),el('span',{class:'small',html:bits.map(b=>`<span class="nw">${b}</span>`).join(' · ')})]))});
    // 扩展：徽章 title 在触屏上看不到，所以把"换了文件未重启 / 待换入"的解释直接写成小字放在各自下面。
    body('extensions').innerHTML=`<div class="kv small">
      <b>${T('health.xovi')}</b><span>${xoviTxt}</span>
      <b>${T('health.extensions')}</b><span>${names(x.extensions)}</span></div>
      ${(x.deleted&&x.deleted.length)||(d.soPending&&d.soPending.length)?`
      <h3>${T('health.deleted')}</h3><p class="small">${T('health.deletedTitle')}</p>
      <p>${x.deleted&&x.deleted.length?`<span class="badge off">${esc(x.deleted.join(' · '))}</span>`:none}</p>
      <h3>${T('health.soPending')}</h3><p class="small">${T('health.soPendingTitle')}</p>
      <p>${d.soPending&&d.soPending.length?`<span class="badge">${esc(d.soPending.join(' · '))}</span>`:none}</p>`
      :`<p class="small">${T('health.extClean')}</p>`}`;
    // 上次开机最后几行 journal（设备冻死/意外重启的线索）；journal 没持久化时后端给 null。
    body('log').innerHTML=`<h3 style="margin-top:0">${T('health.prevBoot')}</h3>`
      +(d.prevBoot&&d.prevBoot.length?`<pre class="hlog">${esc(d.prevBoot.join('\n'))}</pre>`:`<p class="small">${T('health.prevBootNone')}</p>`);
  };
  /* 进这一屏（fresh=false）或点刷新（fresh=true）：健康数据现取；清理只在它正在前台时取，否则等第一次切过去。 */
  const load=async fresh=>{cleanupStale=true;await Promise.all([loadHealth(fresh),cleanupIfShown()])};
  guardClick(refreshBtn,()=>load(true));
  return load;
}
function mountCleanup(box){
  box.innerHTML=`<h2>${T('cleanup.title')}</h2><p class="lead">${T('cleanup.lead')}</p>
    <h3>${T('cleanup.done.title')}</h3><p class="small" data-filesdesc>${T('cleanup.done.desc')}</p>
    <ul class="list" data-files></ul><div class="row" data-filesrow><button class="btn btn-bad" data-delfiles disabled></button></div>
    <h3>${T('cleanup.lib.title')}</h3><p class="small">${T('cleanup.lib.desc')}</p><p class="small" data-agent></p>
    <div class="stg-tools"><input type="search" data-q placeholder="${T('cleanup.lib.search')}"><select data-filter><option value="dup">${T('cleanup.lib.filterDup')}</option><option value="all">${T('cleanup.lib.filterAll')}</option></select></div>
    <ul class="list" data-lib></ul><div class="row" data-librow><button class="btn btn-bad" data-trash disabled></button></div>`;
  const q=s=>box.querySelector(s);
  const pickF=new Set(),pickL=new Map();let files=[],lib=[];
  const syncBtns=()=>{const a=q('[data-delfiles]'),b=q('[data-trash]');
    a.textContent=T('cleanup.deleteBtn',{n:pickF.size});a.disabled=!pickF.size;
    b.textContent=T('cleanup.lib.trashBtn',{n:pickL.size});b.disabled=!pickL.size};
  const check=(on,fn)=>{const c=el('input',{type:'checkbox'});c.checked=on;c.onchange=()=>{fn(c.checked);syncBtns()};return el('label',{class:'stg-check'},[c])};
  const renderFiles=()=>{const ul=q('[data-files]');ul.innerHTML='';
    q('[data-filesdesc]').hidden=q('[data-filesrow]').hidden=!files.length;
    if(!files.length){ul.appendChild(el('li',{class:'small',text:T('cleanup.done.empty')}));return}
    files.forEach(f=>ul.appendChild(el('li',{},[el('span',{style:'display:flex;gap:.5em;align-items:flex-start'},[check(pickF.has(f.name),v=>v?pickF.add(f.name):pickF.delete(f.name)),el('span',{text:f.name})]),
      el('span',{class:'small',text:`${fmtB(f.bytes)} · ${fmtTime(f.mtime)}`})])))};
  const renderLib=()=>{const ul=q('[data-lib]');ul.innerHTML='';
    const kw=q('[data-q]').value.trim().toLowerCase(),dup=q('[data-filter]').value==='dup';
    const list=lib.filter(b=>(!dup||b.sameName>1)&&(!kw||(b.name+' '+b.folder).toLowerCase().includes(kw)));
    q('[data-librow]').hidden=!list.length&&!pickL.size;
    if(!list.length){ul.appendChild(el('li',{class:'small',text:T('cleanup.lib.empty')}));return}
    list.forEach(b=>{
      const meta=[esc(b.folder||T('cleanup.lib.root')),b.kind.toUpperCase(),fmtB(b.bytes),esc(fmtTime(Math.floor(b.createdMs/1000)))].join(' · ');
      const same=b.sameName>1?` <span class="badge" title="${esc(T('cleanup.lib.sameNameTitle',{n:b.sameName}))}">${T('cleanup.lib.sameName',{n:b.sameName})}</span>`:'';
      ul.appendChild(el('li',{},[el('span',{style:'display:flex;gap:.5em;align-items:flex-start;flex:1;margin-left:0'},[check(pickL.has(b.uuid),v=>v?pickL.set(b.uuid,b.name):pickL.delete(b.uuid)),el('span',{html:`${esc(b.name)}${same}<br><span class="small">${meta}</span>`})])]))})};
  q('[data-q]').oninput=renderLib;q('[data-filter]').onchange=renderLib;
  const listOf=names=>names.slice(0,12).map(n=>'· '+n).join('\n')+(names.length>12?'\n…':'');
  /* 不用 guardClick：它在 finally 里无条件解禁按钮，而这里"没勾选"时按钮应保持禁用——收尾交给 syncBtns。 */
  const busyClick=(b,fn)=>{b.onclick=async()=>{if(b.disabled)return;b.disabled=true;try{await fn()}catch(e){console.error(e);toast(T('common.failed'))}finally{syncBtns()}}};
  busyClick(q('[data-delfiles]'),async()=>{const names=[...pickF];
    if(!names.length||!await confirmDialog(T('cleanup.confirmFiles',{n:names.length,list:listOf(names)})))return;
    const r=await jsend('/api/device/cleanup/delete','POST',{area:'books-done',names});
    if(r.failed&&r.failed.length)toast(T('cleanup.partial',{ok:(r.deleted||[]).length,bad:r.failed.length,msg:r.failed.map(f=>T('common.labelValue',{label:f.name,value:f.error})).join(T('common.listSep'))}));
    else if(r.ok===false)toast(r.message||T('common.failed'));
    else toast(T('cleanup.deleted',{n:(r.deleted||[]).length}),'ok');
    pickF.clear();await load()});
  busyClick(q('[data-trash]'),async()=>{const picks=[...pickL];
    if(!picks.length||!await confirmDialog(T('cleanup.lib.confirm',{n:picks.length,list:listOf(picks.map(p=>p[1]))})))return;
    let ok=0;const bad=[];
    for(const [uuid,name] of picks){const r=await jsend('/api/books/trash/add','POST',{uuid,name});if(r.ok===false)bad.push(T('common.labelValue',{label:name,value:r.message||''}));else ok++}
    if(bad.length)toast(T('cleanup.partial',{ok,bad:bad.length,msg:bad.join(T('common.listSep'))}));else toast(T('cleanup.lib.queued',{n:ok}),'ok',6500);
    pickL.clear();await load()});
  const load=async()=>{const d=await j('/api/device/cleanup');
    if(d.ok===false){q('[data-files]').innerHTML=`<li class="small">${esc(d.message)}</li>`;return}
    files=d.files||[];lib=d.library||[];
    for(const n of [...pickF])if(!files.some(f=>f.name===n))pickF.delete(n);
    for(const u of [...pickL.keys()])if(!lib.some(b=>b.uuid===u))pickL.delete(u);
    q('[data-agent]').textContent=!d.xochitl?T('cleanup.lib.noXochitl'):d.trashAgent?'':T('cleanup.lib.agentOff');
    renderFiles();renderLib();syncBtns()};
  syncBtns();
  return load;
}
/* 页头横幅（OTA / WiFi / 代理放弃三种同一套外观）：插在标签栏下、main 前；同 id 已在就原地替换。
   closable：左上角 ×，只在本次页面会话内关掉。 */
const banner=(id,html,closable)=>{const ban=el('div',{class:'otabanner',id,role:'alert',html});
  if(closable)ban.prepend(btn('×',()=>ban.remove(),'btn x',{title:T('ota.dismiss'),'aria-label':T('ota.dismiss')}));
  const old=document.getElementById(id);if(old)old.replaceWith(ban);else document.body.insertBefore(ban,$('#main'));return ban};
/* 页头"需要重新安装"横幅（2026-09-25）：页面打开时取一次 /api/device/ota（网关侧判定见 device/ota.rs：单元文件缺失 /
   xovi 未生效；固件哈希不在白名单只作附加原因），不轮询。关掉只在本次页面会话内有效。 */
async function showOtaBanner(){
  const d=await j('/api/device/ota');if(d.ok===false||!d.needsReinstall)return;
  const reasons=(d.reasons||[]).map(r=>`<li>${esc(T('ota.reason.'+r,{units:(d.missingUnits||[]).join(', ')}))}</li>`).join('');
  const cmd=d.recovery==='full'?`<p>${T('ota.recovery.full',{cmd1:'<code>/home/root/xovi/rebuild_hashtable</code>'})}</p><pre class="hlog">cd packaging &amp;&amp; sh install-all.sh ${esc(location.hostname)}${(d.reasons||[]).includes('firmware-unknown')?' --force':''}</pre>`
    :`<p>${T('ota.recovery.xovi')}</p><pre class="hlog">cd packaging &amp;&amp; sh deploy-xovi-apply.sh ${esc(location.hostname)}</pre>`;
  banner('otabanner',`<b>${d.recovery==='full'?T('ota.banner.title'):T('ota.banner.xoviTitle')}</b><ul>${reasons}</ul>${cmd}<p class="small">${T('ota.recovery.doc')}</p>`,true);
}
/* 页头"这个 WiFi 上不了外网"横幅（2026-09-28）：设备端 wifi-watch 连上新网络时探一次外网，结果经 /api/device/wifi 给这里。
   酒店那种要网页登录的 WiFi 上 xochitl 会反复连云端、设备一直醒着很耗电，而 reMarkable 上没法完成网页登录。页面打开时取一次，不轮询；× 只在本次页面会话内关掉。 */
async function showWifiBanner(){
  const d=await j('/api/device/wifi');if(d.ok===false||(d.state!=='portal'&&d.state!=='none'))return;
  banner('wifibanner',`<b>${esc(T('wifi.banner.'+d.state,{ssid:d.ssid||'?'}))}</b><p>${T('wifi.banner.why')}</p><p class="small">${T('wifi.banner.hint')}</p>`,true);
}
/* 页头"设备上没做成"横幅（2026-09-25）：移进 xochitl 回收站、在 xochitl 书库建文件夹，这两件事由设备端代理执行，
   交满 5 次仍没做成 book-serve 就放弃（见 book-serve agent_failures.rs）。页面打开时取一次，之后收到 `agent-failed`
   事件再取，不轮询；「知道了」清空服务端记录（换台设备/刷新后也不再出现）。book-serve 没开时接口不通，不显示。 */
async function showAgentFailBanner(){
  const d=await j('/api/books/agent-failures');const items=d.ok===false?[]:(d.items||[]);
  if(!items.length){const old=$('#agentfail');if(old)old.remove();return}
  const li=items.slice().reverse().map(f=>`<li>${esc(T('agentfail.'+(f.kind==='mkdir'?'mkdir':'trash'),{name:f.name}))} <span class="small">${esc(fmtTime(f.at))}</span></li>`).join('');
  const ban=banner('agentfail',`<b>${T('agentfail.title',{n:items.length})}</b><ul>${li}</ul><p class="small">${T('agentfail.hint')}</p>`);
  ban.appendChild(btn(T('agentfail.ack'),async()=>{if((await postJ('/api/books/agent-failures/clear',{})).ok!==false)ban.remove()}));
}
