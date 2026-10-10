/* 标签栏吸顶位置跟着页头实际高度（手机英文界面页头会折两行），见 style.css nav 的 --hdr-h。只在尺寸变化时回调，不轮询。 */
if(window.ResizeObserver)new ResizeObserver(([e])=>document.documentElement.style.setProperty('--hdr-h',Math.ceil(e.target.getBoundingClientRect().height)+'px')).observe($('header'));
$('#logout').onclick=e=>{e.preventDefault();fetch('/logout',{method:'POST'}).then(()=>location.href='/login',()=>toast(T('common.networkError')))};
// 徽章的完整解释（加入失败原因、渲染状态…）以前只写进 title——触屏摸不到 hover，只看得见图标+数字，看不见
// "为什么/该怎么办"（2026-09-09 审计发现）。这里全局委托一个点击处理：任何带 title 的徽章点一下就弹一条提示显示
// 完整内容，不用逐个改模板字符串。`.badge[title]` 的 `cursor` 在 style.css 里配套改成 help，给一个"这能点"的视觉提示。
document.addEventListener('click',e=>{const b=e.target.closest('.badge[title]');if(b&&b.title)toast(b.title,'info',6500)});

(async()=>{
  // 语言包先拿到手：下面 addTab 用得到 T()，晚拿会让顶层导航先短暂显示 key 本身再跳成文字。
  // 拿不到（离线/服务重启中）静默留空对象——T() 兜底显示 key，不是白屏，也不阻塞页面其余部分。
  const lang=currentLang();
  document.documentElement.lang=lang; // 读屏与字体回退按真实界面语言走（index.html 写死的是 zh）
  try{I18N=await(await fetch(`/ui/locales/${lang}.json`)).json()}catch{I18N={}}
  document.title=T('app.title');$('#applogo').textContent=T('app.title');
  $('#navpw').textContent=T('nav.changePassword');$('#navca').textContent=T('nav.caCert');$('#logout').textContent=T('nav.signOut');
  $('#mainloading').textContent=T('main.loading');
  showOtaBanner(); // 不 await：横幅晚一点出现无妨，不挡页面主体
  showAgentFailBanner();
  showWifiBanner();
  const langsel=$('#langsel');langsel.value=lang;langsel.setAttribute('aria-label',T('nav.lang'));
  langsel.onchange=()=>{LS.set('lang',langsel.value);location.reload()};

  const d=await j('/api/services');
  // service→seg：网关从 manage.rs::MODULES 派生、随服务列表带回（没有 seg 就退回用服务名本身当 area，
  // 跟下面两处 `AREA[s.name]||s.name` 的 fallback 语义一致）。
  const AREA=Object.fromEntries((d.services||[]).filter(s=>s.seg).map(s=>[s.name,s.seg]));
  const svcs=(d.services||[]).filter(s=>s.ui&&TABS[s.name]).sort((a,b)=>a.ui.order-b.ui.order);
  /* 首层标签顺序（2026-09-10 用户重排）：传书 / 笔记 / 其他 / 管理。笔记单独占位，xochitl(font-serve)/
     壁纸(wallpaper-serve)——目前 svcs 里除笔记外还带 ui.order 的候选——一律降一级包进「其他」（见 renderOther）。 */
  const noteSvc=svcs.filter(s=>s.name==='note-serve');
  const otherSvcs=svcs.filter(s=>s.name!=='note-serve');
  $('#hdr').textContent=location.host;
  const nav=$('#tabs'),main=$('#main');main.innerHTML='';
  const secByArea={};
  /* 各 tab **第一次切过去时才渲染**（渲染本身就会取一次数据）：此前页面一打开就把笔记/其他/管理全部渲染、各自取一遍数据
     （二十来个请求，还让网关扫 /proc、问模型服务），而且首个 tab 渲染完紧接着又被点击刷新一次，同样的 6 个请求发两遍。
     之后再切回来才走 refreshSec 刷新。 */
  const addTab=(title,render,first,area)=>{const b=document.createElement('button');b.textContent=title;const sec=document.createElement('section');secByArea[area]=sec;
    let rendered=false;
    b.onclick=()=>{[...nav.children].forEach(x=>x.classList.remove('on'));[...main.children].forEach(x=>x.classList.remove('on'));b.classList.add('on');sec.classList.add('on');
      if(!rendered){rendered=true;render(sec)}else if(sec.refresh)refreshSec(sec)};
    nav.appendChild(b);main.appendChild(sec);if(first)b.onclick();return sec};
  addTab(T('tab.transfer'),renderTransfer,true,EV.area.BOOKS);          // 总入口（入库｜母版库），固定第一位（book-serve 不在时列表里提示去管理页开）
  noteSvc.forEach((s)=>addTab(T(TABS[s.name].titleKey),TABS[s.name].render,false,AREA[s.name]||s.name));
  if(otherSvcs.length){
    // 'other' 不是事件 area，只是这个 section 在 secByArea 里的占位键；字体/壁纸的 area 在下面指到它。
    const otherSec=addTab(T('tab.other'),(sec)=>renderOther(sec,otherSvcs,n=>AREA[n]||n),false,'other');
    // fonts/wallpapers 的 SSE 事件都指向同一个「其他」section，由它的 onEvent 只刷发事件的那块子面板
    // （2026-09-25 起）；切到「其他」tab、重连时才几块一起刷。
    otherSvcs.forEach(s=>{secByArea[AREA[s.name]||s.name]=otherSec});
  }
  addTab(T('tab.manage'),renderManage,false,EV.area.MANAGE);            // 固定管理台，始终可进
  /* 事件推送（SSE，零轮询）：服务在变更处发事件 → 网关 /api/events 汇聚 → 这里只刷对应 tab；不在前台的 tab 不管，切过去时本来就刷。
     manage 事件（服务启停）：tab 集合变了就整页重载，否则只刷管理台。断线（WiFi 掉/设备休眠醒来）EventSource 自动重连，
     重连成功（非首次 onopen）补刷一次当前 tab——断线期间的事件没人推给我们。
     省电/省流量：页面被隐藏（切标签页、手机锁屏）时**不刷新**，只记"当前 tab 待刷"，`visibilitychange` 变可见时补刷一次；
     每个 tab 的刷新用 coalesce 合并（事件突发/多来源叠加时同一时刻最多一个在飞）。 */
  const svcKey=svcs.map(s=>s.name).join(',');
  const liveDot=el('span',{id:'live',title:T('common.eventStream'),text:'●'});liveDot.style.cssText='margin-left:.5em;font-size:.8em;color:var(--bad)';$('#hdr').appendChild(liveDot);
  const activeSec=()=>[...main.children].find(x=>x.classList.contains('on'));
  let activeStale=false,opened=false,es=null,hiddenTimer=0;
  /* 服务集合变了（装/卸/启停带来 tab 增减）就整页重载。只在页面可见时查：隐藏时只记 `svcStale`，变可见再查一次——
     此前隐藏时每个 manage 事件都照样去取 /api/services；而隐藏超过 60 秒 SSE 断开期间漏掉的 manage 事件，重连后也没人补查，
     tab 列表会一直停在旧的服务集合上。 */
  let svcStale=false;
  const checkServices=async()=>{if(document.hidden){svcStale=true;return false}svcStale=false;
    const d=await j('/api/services');if(d.ok===false)return false;
    const k=(d.services||[]).filter(s=>s.ui&&TABS[s.name]).map(s=>s.name).join(',');
    if(k!==svcKey){location.reload();return true}return false};
  /* 心跳 ?ka=60：默认 20 秒一帧空注释，只是为了让中间代理不掐空闲连接；网关直连浏览器用不着这么勤（设备上每帧都是一次唤醒+TLS 写）。
     页面隐藏超过 60 秒就**主动断开** SSE（锁屏/切走的标签页不再让设备为它保活），重新可见时重连——重连成功的 onopen
     本来就会补刷当前 tab（见上），断开期间漏掉的事件不丢；别的 tab 切过去时无条件刷新（addTab 的点击处理）。
     **浏览器放弃重连的情况**：连上时回的不是 200（网关重启后内存里的会话全没了→401、并发满→503），EventSource 直接进
     CLOSED、再也不重试——此前页面就此静默失去实时刷新（红点一直亮，用户不点东西就不知道要重新登录）。现在 CLOSED 时
     先查一次 /api/session（401 由 j() 带去登录页），其余情况按 5 秒起、翻倍、封顶 5 分钟的退避重开；页面隐藏时不重试，
     等变可见时由 visibilitychange 重开。浏览器自己在重连的（CONNECTING）不插手。 */
  const HIDDEN_CLOSE_MS=60000,RETRY_MIN_MS=5000,RETRY_MAX_MS=300000;
  let retryMs=0,retryTimer=0;
  const reopenLater=()=>{clearTimeout(retryTimer);if(document.hidden)return;
    retryMs=Math.min(retryMs?retryMs*2:RETRY_MIN_MS,RETRY_MAX_MS);
    retryTimer=setTimeout(async()=>{if(es||document.hidden)return;
      const s=await j('/api/session');if(s.ok===false){reopenLater();return} // 401/403 时 j() 已经跳走
      if(!es&&!document.hidden)openEs()},retryMs)};
  const openEs=()=>{clearTimeout(retryTimer);
    const src=es=new EventSource('/api/events?ka=60');
    es.onopen=()=>{retryMs=0;liveDot.style.color='var(--ok)';liveDot.title=T('common.eventStreamConnected');
      // 重连（非首次）：断线期间的事件没人推给我们——补查服务集合（可能整页重载）、补刷当前 tab。
      if(opened){svcStale=true;if(document.hidden)activeStale=true;else(async()=>{if(await checkServices())return;activeStale=false;const s=activeSec();if(s)refreshSec(s)})()}opened=true};
    es.onerror=()=>{liveDot.style.color='var(--bad)';liveDot.title=T('common.eventStreamReconnecting');
      if(src.readyState===2&&es===src){closeEs();reopenLater()}}; // 2 = EventSource.CLOSED：浏览器不会再自己重连
    es.onmessage=async(e)=>{let ev;try{ev=JSON.parse(e.data)}catch{return}
      if(ev.kind===EV.kind.AGENT_FAILED){showAgentFailBanner();return} // 全站横幅，与哪个 tab 无关
      if(ev.area===EV.area.MANAGE&&await checkServices())return;
      const sec=secByArea[ev.area];if(!sec)return;
      if(!sec.classList.contains('on'))return;
      if(document.hidden)activeStale=true;
      else if(sec.onEvent)sec.onEvent(ev); // tab 自己按事件决定刷多少（母版库：网关排队/进度事件只重取两个状态）
      else refreshSec(sec)}};
  const closeEs=()=>{if(!es)return;es.close();es=null;liveDot.style.color='var(--bad)';liveDot.title=T('common.eventStreamReconnecting')};
  document.addEventListener('visibilitychange',()=>{
    if(document.hidden){clearTimeout(hiddenTimer);hiddenTimer=setTimeout(closeEs,HIDDEN_CLOSE_MS);return}
    clearTimeout(hiddenTimer);
    if(!es){retryMs=0;openEs();return} // 重连后的 onopen 负责补查服务集合、补刷当前 tab
    (async()=>{if(svcStale&&await checkServices())return;
      if(activeStale){activeStale=false;const s=activeSec();if(s)refreshSec(s)}})()});
  openEs();
  if(document.hidden)hiddenTimer=setTimeout(closeEs,HIDDEN_CLOSE_MS); // 页面是在后台标签页里打开的
})();
