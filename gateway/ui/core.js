/* HTML 转义：**所有外部数据**（文件名、字体内部名、书里的划线/手写转写文本、AI 回答、服务端错误文案）插进
   innerHTML/insertAdjacentHTML/属性值之前必须过它。文件名允许含 `<`（rmsvc_core::fs::plain_name 只拒 `/` `\` 和
   开头的 `.`），书里的元数据、字体 name 表、OCR/大模型输出也都是外部内容——不转义就是存储型 XSS。
   textContent/el({text}) 天然安全，不需要它。 */
const esc=s=>String(s==null?'':s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const fmtB=n=>n>1048576?(n/1048576).toFixed(1)+' MB':n>1024?(n/1024).toFixed(0)+' KB':n+' B';
// 母版库剩余空间告急：优先用 book-serve 给的 lowSpace（阈值由后端定）；旧版 book-serve 没有这个字段时退回前端按 300MB 判。
const stagingLowSpace=d=>typeof d.lowSpace==='boolean'?d.lowSpace:d.freeBytes!=null&&d.freeBytes<300*1048576;
/* 停一会儿再继续：用在"先弹出一条状态文字，再触发会重画掉这条文字的动作"这种场景——不等的话状态
   文字刚显示就被紧跟着的重画冲掉，用户根本来不及看见（点重转/生成笔记本弹出消耗那次踩过的坑）。 */
const wait=ms=>new Promise(res=>setTimeout(res,ms));
/* 合并并发刷新：包一个异步刷新函数，同一时刻最多一个在飞；跑的时候又被调用只记一个"跑完再来一次"，多次调用合成
   一次。SSE 事件突发、切 tab、别的触发源叠在一起时不会并发出一堆请求，旧响应也不会盖过新响应（串行执行，
   结果按顺序落地）。返回的 Promise 在"包含本次调用之后的那一轮"结束时完成。 */
const coalesce=fn=>{let running=null,again=false;
  const loop=async()=>{do{again=false;try{await fn()}catch(e){console.error(e)}}while(again)};
  return()=>{if(running){again=true;return running}running=loop().finally(()=>{running=null});return running}};
/* 一个区块（顶层 tab 或「其他」里的子面板）的刷新统一走它（切 tab、SSE、可见性恢复）：按区块各自 coalesce，合并并发触发。 */
const refreshSec=sec=>{if(!sec.refresh)return;(sec._rf||(sec._rf=coalesce(async()=>{await sec.refresh()})))()};
/* 轻量记忆：per-viewer 便利态，隐私窗口/禁用 storage 时静默回默认 */
const LS={get(k,d){try{const v=localStorage.getItem('shelf.'+k);return v==null?d:v}catch{return d}},set(k,v){try{localStorage.setItem('shelf.'+k,v)}catch{}}};
// 函数而不是加载时求值的常量：core.js 加载时不碰 location/document（node 测试整文件加载它）。
const onUsb=()=>/^10\.11\.99\./.test(location.hostname);
/* i18n 架子（2026-09-09 审计新增；2026-09-10 补全正文全覆盖）。登录页/改密码页（`src/ui.rs` 的
   `login_page`/`password_page`）不在这次范围内——那两页是 Rust 端独立 `format!` 拼字符串，未登录态
   不跑这份 JS、读不到 `LS`（localStorage）里的语言选择，要做需要另一套"未登录态也能传语言"的机制
   （比如写 cookie 给 Rust 端读），跟这里的做法不是一回事，留给以后真有需要再做。`I18N` 在启动 IIFE
   里异步填充，填充完成之前 `T()` 兜底显示 key 本身（不留空白，也不会悄悄掩盖翻译缺口）。
   ⚠️ `T()` 只能在"渲染/交互时才执行"的函数体里调用——`I18N` 是异步填充的，如果把 `T()` 调用塞进
   模块顶层 `const 模板字符串="..."` 这种脚本解析时就立即求值一次、以后不会重新求值的地方，结果会被
   烤死成 key 兜底文本，永远显示不出真翻译，还不报错（`GUIDE` 改成零参数函数
   就是为了避开这个坑，见各自定义处）。 */
let I18N={};
/* vars：可选的 {占位符名: 值} 插值表，key 对应的文案里用 {占位符名} 占位，逐个字符串替换（次数少，
   用 split/join 够用，不必上正则）。零参数调用（`T('xxx')`）行为跟以前完全一样。 */
const T=(key,vars)=>{let s=I18N[key]||key;if(vars)for(const k in vars)s=s.split('{'+k+'}').join(vars[k]);return s};
const currentLang=()=>LS.get('lang',(navigator.language||'').toLowerCase().startsWith('en')?'en-US':'zh-CN');
/* 格式白名单：服务端 rmsvc_core::formats 注入（同一份，网页 accept + 选中即拦 = 服务端上传门） */
const EXT=__EXTS__, dot=l=>l.map(e=>'.'+e);
const BOOK_EXT=dot(EXT.book), FONT_EXT=dot(EXT.font), IMG_EXT=dot(EXT.image);
const up=l=>l.map(e=>e.toUpperCase()).join(' / ');
/* 母版库一本书的格式徽章文字。book-serve 只认 EPUB/PDF，其余文件（格式收窄前留下的 .cbz 等）报 `format:"other"`——
   此前这里写死"不是 pdf 就是 EPUB"，残留的 CBZ 也标成 EPUB（2026-10-10 修）。白名单里的格式照 EXT.book 显示；
   其余显示文件真实扩展名，取不到扩展名才用"其他"。 */
const bookFmtLabel=it=>{if(EXT.book.includes(it.format))return it.format.toUpperCase();
  const m=/\.([^./]+)$/.exec(it.name||'');return m?m[1].toUpperCase():T('stg.fmt.other')};
// 2026-09-18 起母版库只收 EPUB/PDF（网关只注入 EXT.book，见 rmsvc_core::formats 头注）——原来
// 这里有个 FMT_TIERS() 分两档（原生/仅 KOReader）拼文案，两档收成一档后不再需要，删掉。
// 响应不是合法 JSON（网关自身 502/504、反代错误页…）时，以前直接把裸状态码当 message 弹给用户
// （"HTTP 502"），技术术语没翻译成人话（2026-09-09 审计发现）。改成一句人话+状态码放在括号里，
// 报障时还能带出这个号。
// 网络层异常（设备休眠、WiFi 断开、网关重启中）fetch 直接抛 TypeError——此前没接住，开关一直灰着、按钮没反应、
// 用户看不到任何提示（2026-09-24 审查）。统一在这里转成 {ok:false,message}，调用方照常按失败处理。
/* 401/403 统一去向（j() 与上传器的 XHR 共用）：登录过期 → 登录页（带回当前页）；首登未改密 → 改密页。处理了返回提示文案，否则 null。
   403 只来自网关登录守卫（首登必改）——各领域服务都不回 403，所以不必再看应答体。 */
const authRedirect=status=>{if(status===401){location.href='/login?next='+encodeURIComponent(location.pathname);return T('common.needLogin')}if(status===403){location.href='/password';return T('common.needChangePassword')}return null};
async function j(url,opt){let r;try{r=await fetch(url,opt)}catch(e){console.error(e);return {ok:false,message:T('common.networkError')}}const auth=authRedirect(r.status);if(auth)return {ok:false,message:auth};
  const httpErr=T('common.httpErr',{status:r.status});
  let d;try{d=await r.json()}catch{d={ok:false,message:httpErr}}if(!r.ok&&d.ok!==false)d={ok:false,message:d.message||httpErr};return d}
/* 带 JSON body 的请求：`jsend(url,'PUT',{a:1})`；省略 body＝不带请求体（`jsend(url,'POST')`）。全站非 GET 调用共用。 */
const jsend=(url,method,body)=>j(url,body===undefined?{method}:{method,body:JSON.stringify(body)});
/* 同 jsend，失败时顺手弹 toast（服务端给的 message，没有就用 failKey 的文案），返回应答照常给调用方判断。
   全站"发请求 → 失败提示"都走它（postJ / bindToggle / 模型面板 / 删除按钮…），不再各处手写同一句。 */
const sendT=async(url,method,body,failKey='common.failed')=>{const r=await jsend(url,method,body);if(r.ok===false)toast(r.message||T(failKey));return r};
const postJ=(url,body)=>sendT(url,'POST',body);
/* SSE 事件的 area/kind/tab 取值（`/api/events`，网关汇聚各服务的 `bus.publish(area, kind)` 后补上 `svc` 与 `tab`）。网页按 tab
   找顶层 tab（`secByTab`；tab 由网关 manage.rs::MODULES 声明，2026-10-10 前按 area 找，ink/transcribe 发 area:'notes' 能到笔记页
   只因 note-serve 的段恰好叫 notes）、按 svc 分「其他」里的子面板、按 kind 决定刷多少；所有分支都用这张表，不写字面量。
   产生方与各取值的含义见网关白皮书「事件 area/kind 总表」。
   服务侧：网关（events.rs）、book-serve（events.rs）、笔记线三个服务（各自 main.rs 的 EVENT_* 常量）都是常量，网关与 book-serve 的测试
   核对本表认得它们发的每个取值；font-serve、wallpaper-serve 仍写字面量（取值少、只在本服务里）。改名时两边都要动。 */
const EV={
  area:{BOOKS:'books',NOTES:'notes',FONTS:'fonts',WALLPAPERS:'wallpapers',MANAGE:'manage'},
  tab:{BOOKS:'books',NOTES:'notes',OTHER:'other',MANAGE:'manage'},      // 顶层 tab：传书 / 笔记 / 其他 / 管理
  kind:{
    STAGING:'staging',RENDER:'render',MKDIR:'mkdir',TRASH:'trash',INBOX:'inbox',IMPORT:'import',AGENT_FAILED:'agent-failed', // book-serve
    BATCH:'batch',SERVICES:'services',                                    // 网关自己（批量队列 / 服务注册表变化）
    ENTRIES:'entries',TRANSCRIBE:'transcribe',NOTEBOOKS:'notebooks',      // ink-serve / transcribe-serve / note-serve（area 都是 notes）
    FONTS:'fonts',UI:'ui',CONFIG:'config',POOL:'pool',                    // font-serve / wallpaper-serve
  },
};
/* 笔记页的刷新闸门（2026-10-10 把 renderNotes 里散落的六组时序标志收成这一个对象；机制与各道闸的条件都没变）。SSE 事件、
   切 tab、换书、自己的动作都会要求"重取整本书并重画"，三种情况要挡：
   ① hold（挡多久 + 为什么）：刚显示出一条结果提示（推送本章 / 重新转写 / 提问的"✓ 完成 · token…"），或那个请求还在跑。
      这几个动作本身会让 ink-serve 发 entries 事件，SSE 触发的整段重画比提示停留时间快得多，提示一闪就被冲掉（真机反馈
      "重复推送的提示看不清"，2026-09-17；把停留从 1.5s 延到 3s 没用，根子在被抢跑）；请求还在跑时重画还会让结果写到
      已经脱离页面的节点上。所以请求在跑时挡 HOLD_BUSY_MS（10 分钟只是兜底），结果出来后改成再挡 LINGER_MS，之后调用方
      自己主动重画一次——不会漏刷新。只挡 SSE 触发的刷新（含只刷同步状态的那条）；用户自己换书、清空回收站是 force，不挡。
   ② quiet：自己的动作刚重取过整本书（reloadBook），同一动作让服务端发出的 entries 事件再整页刷新就是重复取——重取期间与之后
      SELF_QUIET_MS 内的 entries 事件不理。
   ③ editing：用户正在本 tab 的输入框里打字，事件来了先不重画（光标会连同输入框一起被重画掉），记一笔，焦点离开再补一次。
   另外合并"下一轮刷新要取什么"：list（书列表）、checkImport（「导入 md」开关，只有切回本 tab 时查）、force（不受 hold 挡）。
   被 hold 挡掉的那一轮：force 已消耗，list/checkImport 留给下一轮（与改造前一致）。
   `st` 记着当前各道闸的状态与最近一次判定（last），出问题时在控制台看它就知道是哪道闸挡的。纯逻辑、时间可注入，见 core.test.mjs。 */
const NOTE_GATE={HOLD_BUSY_MS:10*60*1000,LINGER_MS:3000,SELF_QUIET_MS:1000};
const refreshGate=(now=()=>Date.now())=>{
  const st={holdUntil:0,holdWhy:'',quietUntil:0,deferred:false,want:{list:false,checkImport:false,force:false},last:''};
  const held=()=>now()<st.holdUntil;
  return {st,held,
    /* 挡 SSE 触发的刷新 ms 毫秒（0 = 解除）；why 只用来记录。 */
    hold(ms,why){st.holdUntil=ms?now()+ms:0;st.holdWhy=ms?why:''},
    quietBegin(){st.quietUntil=Infinity},
    quietEnd(){st.quietUntil=now()+NOTE_GATE.SELF_QUIET_MS},
    /* 一个 SSE 事件该怎么处理：'defer'（正在输入，记一笔）/ 'sync'（只刷同步状态）/ 'quiet'（自己刚重取过，不理）/ 'refresh'。 */
    event(kind,editing){
      if(editing){st.deferred=true;return st.last='defer'}
      if(kind===EV.kind.NOTEBOOKS)return st.last='sync';
      if(kind===EV.kind.ENTRIES&&now()<st.quietUntil)return st.last='quiet';
      return st.last='refresh'},
    /* 焦点离开输入框后：之前有被挡下的事件就该补一次刷新。 */
    blurred(editing){if(st.deferred&&!editing){st.deferred=false;return true}return false},
    request(list,checkImport,force){const w=st.want;w.list=w.list||list;w.checkImport=w.checkImport||checkImport;w.force=w.force||force},
    /* 一轮刷新开始时取走请求；被 hold 挡住返回 null。 */
    take(){const w=st.want,force=w.force;w.force=false;
      if(!force&&held()){st.last='hold:'+st.holdWhy;return null}
      const r={list:w.list,checkImport:w.checkImport,force};w.list=false;w.checkImport=false;st.last='run';return r},
  };
};
