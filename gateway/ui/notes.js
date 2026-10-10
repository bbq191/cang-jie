/* 「笔记」tab（note-serve 注册；数据来自 ink-serve 条目库）：按书→按章列条目，左裁图右文本，改即存。
   设备只负责写、不负责改：这里就是"改"的地方（e-ink 上打字太痛苦）。三期（2026-09-08）砍掉了"分区"——
   AI 触发早就是按条目单发（勾「问AI」+ 填问题+点提问，调 mind-serve 拼"书名+章节+勾画原文+转写文本+问题"
   发模型，二期，白皮书 §03n），分区兼职的笔记本排版分组也不要了，条目一律按页序平铺，格式=Entry.style。 */
// 顶层常量只放 i18n key 名，不放翻译好的文字——真正的 T() 查找挪到调用点（渲染时执行），见 T() 头注的硬性规则。
const STYLE_NAMES={body:'notes.style.body',bullet:'notes.style.bullet',numbered:'notes.style.numbered',checkbox:'notes.style.checkbox',heading1:'notes.style.heading1',heading2:'notes.style.heading2'};
/* archived 显示"已删除（可恢复）"而不是单纯"已删除"：这是软删终态，回收站里随时能点「恢复」，
   跟「清空回收站」那个真正不可逆的操作不该用同一个不加限定的"删除"措辞（2026-09-09 审计修：原文案
   会让用户误以为回收站里这条已经彻底没了）。 */
const STATUS_NAMES={mined:'notes.status.mined',pending:'notes.status.pending',draft:'notes.status.draft',reviewed:'notes.status.reviewed',skipped:'notes.status.skipped',revoked:'notes.status.revoked',archived:'notes.status.archived'};
/* Obsidian 官方图标是紫色多面体"石头"，不是随便一个链接符号——真机反馈"能否用它自己的图标"，用一个
   简化的多面体 SVG（不是官方 logo 的精确描边，商标图形不该随手照抄，这个形状+配色足够让人一眼认出
   "这是 Obsidian"）替掉原来占位的 🔗。设备笔记本用 📓 emoji 就够直观，不用特别做图标。 */
const OBSIDIAN_ICON='<svg viewBox="0 0 24 24" width="13" height="13" style="vertical-align:-2px;flex:none" aria-hidden="true"><path d="M12 2 19 7.5 17 15 12 22 7 15 5 7.5Z" fill="#8b6cef"/></svg>';
// 混了 emoji/SVG 和文字，值改成惰性函数（调用时才 T()）而不是 key 字符串——跟上面两个字典处理方式
// 不同，是因为这里图标本身要拼进结果里，不是"整串都是 key 对应的文字"。
const DEST_ICON={notebook:()=>'📓 '+T('notes.dest.notebook'),obsidian:()=>OBSIDIAN_ICON+' '+T('notes.dest.obsidian'),both:()=>'📓'+OBSIDIAN_ICON+' '+T('notes.dest.both')};
const DEST_ORDER=['notebook','obsidian','both'];
/* 条目状态的显示名（徽章用，已是 HTML 安全文本）：表里没有的新状态原样显示（转义）。 */
const statusName=e=>STATUS_NAMES[e.status]?T(STATUS_NAMES[e.status]):esc(e.status);
/* 落设备笔记本/落 Obsidian/两处都要（三期，白皮书 §03n 之后）：缺省 both；第二轮反馈把下拉换成
   条目卡片里的循环图标按钮（`DEST_ICON`/`DEST_ORDER`，见 renderBook）。 */
/* 「浏览」（新批注先落这，点了才转笔记）与「整理」（真被要求转笔记的才在这核对）拆两个子视图，见二期设计（白皮书 §03n）。 */
function renderNotes(sec){sec.innerHTML=`
  <div class="card"><h2>${T('notes.title')}</h2>
    <p class="lead">${T('notes.lead')}</p>
    <div class="row"><span class="small">${T('notes.bookLabel')}</span><select id="nbook" style="flex:1;min-width:10em"></select><button class="btn" id="nrescan" title="${T('notes.rescanTitle')}">${T('notes.rescanBtn')}</button></div>
    <div class="row small" id="nsum"></div>
    <div class="row"><input type="search" id="nq" placeholder="${T('notes.search.placeholder')}" aria-label="${T('notes.search.placeholder')}" style="flex:1;min-width:10em"><button class="btn" id="nqgo">${T('notes.search.btn')}</button></div>
    <div id="nqres"></div>
  </div>
  <div class="subnav" id="nsubnav"><button class="on">${T('notes.subnav.browse')}</button><button>${T('notes.subnav.organize')}</button><button>${T('notes.subnav.trash')}</button><button hidden>${T('notes.subnav.import')}</button></div>
  <div class="subpanel on" id="nbrowse"></div>
  <div class="subpanel" id="norganize">
    <div class="subnav" id="nexporttabs"><button class="on" data-etab="pending">${T('notes.export.pending')}</button><button data-etab="synced">${T('notes.export.synced')}</button></div>
    <div class="subnav" id="nchaptertabs"></div>
    <div id="nchapterbody"></div>
  </div>
  <div class="subpanel" id="ntrashpanel">
    <div class="card">
      <p class="lead">${T('notes.trash.lead')}</p>
      <div class="row"><button class="btn" id="npurge" title="${T('notes.trash.purgeTitle')}">${T('notes.trash.purgeBtn')}</button><button class="btn" id="nrestoreall" title="${T('notes.trash.restoreAllTitle')}">${T('notes.trash.restoreAllBtn')}</button><span class="small" id="ntrashsum"></span></div>
      <div id="ntrashlist"></div>
    </div>
  </div>
  <div class="subpanel" id="nimport" hidden>
    <div class="card">
      <p class="lead">${T('notes.import.lead')}</p>
      <div class="row"><span class="small">${T('notes.import.docNameLabel')}</span><input type="text" id="nimporttitle" placeholder="${T('notes.import.docNamePlaceholder')}" style="flex:1;min-width:12em"></div>
      <div class="row"><input type="file" id="nimportfile" accept=".md,.markdown"></div>
      <div class="row small" id="nimportfilename"></div>
      <div class="row"><button class="btn" id="nimportbtn">${T('notes.import.btn')}</button><span class="small" id="nimportstat"></span></div>
    </div>
  </div>`;
  // 当前书的后端路径：`bookApi('ink')` → `/api/ink/books/<uuid>`，`bookApi('ink',`/entries/${id}`)` 带后缀（读 `book`，调用时求值）。
  const bookApi=(svc,sub='')=>`/api/${svc}/books/${encodeURIComponent(book.uuid)}${sub}`;
  // 当前书里某一条目的后端路径：`entryApi('ink',id)` → `/api/ink/books/<uuid>/entries/<id>`，`entryApi('mind',id,'/ask')` 带后缀。
  const entryApi=(svc,id,sub='')=>bookApi(svc,`/entries/${encodeURIComponent(id)}${sub}`);
  // 条目状态动作（ink-serve：request/skip/archive/restore）：POST 空体，失败 postJ 已弹提示，返回是否成功。
  const entryAct=async(id,action)=>(await postJ(entryApi('ink',id,'/'+action),{})).ok!==false;
  const sel=$('#nbook',sec),chaptertabs=$('#nchaptertabs',sec),chapterbody=$('#nchapterbody',sec),browse=$('#nbrowse',sec),sum=$('#nsum',sec);let book=null;
  // 刷新闸门：结果提示停留 / 自己刚重取过 / 正在输入三道闸与"下一轮取什么"，规则见 core.js 的 refreshGate。
  const gate=refreshGate();
  /* 结果文案停留 LINGER_MS（这期间 SSE 触发的重画被闸门挡住），再执行 fn（通常是拉新数据重画）。三处「点完显示结果」共用。 */
  const lingerThen=async fn=>{gate.hold(NOTE_GATE.LINGER_MS,'linger');await wait(NOTE_GATE.LINGER_MS);await fn()};
  /* 「重新转写」/「提问」共用：按钮禁用 + 状态文字（转写中…/提问中…）→ POST 调模型服务 → 显示"✓ 完成 · token 入X 出Y"或错误，
     停留 3s 后重取整本书重画。`reloadOnFail`：失败也重取（转写失败要刷新失败标记）；否则失败只解除暂停、不重画。 */
  const callModel=async(button,stat,url,busyKey,failKey,doneKey,reloadOnFail)=>{button.disabled=true;stat.textContent=T(busyKey);gate.hold(NOTE_GATE.HOLD_BUSY_MS,'busy');
    const r=await j(url,{method:'POST'});
    button.disabled=false;
    stat.textContent=r.ok===false?'✗ '+(r.message||T(failKey)):T(doneKey,{promptTokens:r.promptTokens||0,completionTokens:r.completionTokens||0});
    if(r.ok===false&&!reloadOnFail){gate.hold(0);return}
    await lingerThen(()=>reloadBook(renderBook))};
  const cropUrl=(uuid,f)=>`/api/ink/books/${encodeURIComponent(uuid)}/crops/${encodeURIComponent(f)}`;
  // 2026-09-16 截图走查发现：`e.ink` 有值但 `e.ink.crop` 是空串（ink-serve 自渲染裁图失败/写盘失败时
  // 会发生，见 ingest.rs 的 render_ink/write_atomic 错误分支，只记服务端日志、条目照常落盘）此前被
  // cropHtml 误判成"纯勾画没有手写"（notes.noCrop），实际上这条明明有手写，只是裁图暂时没生成——
  // 两种情况分开提示，别让用户误以为手写没被识别到。
  const cropHtml=e=>e.ink&&e.ink.crop?`<img src="${cropUrl(book.uuid,e.ink.crop)}" alt="${T('notes.cropAlt')}">`:`<div class="empty">${T(e.ink?'notes.cropMissing':'notes.noCrop')}</div>`;
  // 在途的保存请求：重取数据前先等它们落地（失焦保存 `onchange` 不 await，紧跟着的重画可能先拿到旧文本）。
  // 用 jsend 不用 postJ：postJ 失败时自己弹一次 toast，这里再弹"保存失败"就成了两条（此前如此）。
  const inflight=new Set();
  const patch=(id,body)=>{const p=jsend(entryApi('ink',id),'POST',body).then(r=>{if(r.ok===false){toast(r.message||T('notes.saveFailed'));return false}return true}).finally(()=>inflight.delete(p));inflight.add(p);return p};
  /* 编辑区文本失焦才存（`onchange`），但点旁边的按钮（重转/去处/问AI…）会先让文本框失焦触发保存，
     两件事几乎同时各发一个 HTTP 请求，谁先到服务端不一定——按钮那次的收尾动作会拉新数据整页重画，
     如果保存请求还没落地，重画拿到的还是旧文本，编辑就跟着"消失"了（用户反馈"改了内容点重转不存"）。
     用一个 pendingText 记住"还没确认存上"的最新值，任何会拉新数据重画的动作之前先 flush 一遍，
     保证读到的一定是最新的。 */
  const pendingText=new Map();
  const flushPendingText=async()=>{if(book&&pendingText.size){const items=[...pendingText];pendingText.clear();for(const[id,val]of items)await patch(id,{text:val})}
    if(inflight.size)await Promise.all([...inflight])};
  /* 每章"设备笔记本/Obsidian md 是不是已经跟当前条目内容同步"（整理区第三轮反馈）：一次性取整本书
     的同步状态，章头徽章、「整理」列表默认收起已同步章节、回收站显示这条大概去哪了，三处共用同一份，
     不用各自发请求。`refreshSync()` 在 loadBook / reloadBook 里、以及每次生成/导出动作之后调用刷新。
     顺带取转写失败清单（`failedIds`，「整理」里标红 + 按钮改「重转失败」）：此前 renderBook 每画一次就查一次
     /api/transcribe/status，连点章节标签、切「未导出/已导出」这种纯本地切换也要打一次请求。 */
  let syncMap=new Map(),failedIds=new Set();
  const refreshSync=async()=>{if(!book){syncMap=new Map();failedIds=new Set();return}
    const [r,t]=await Promise.all([j(bookApi('notes',`/sync`)),j('/api/transcribe/status')]);
    syncMap=new Map((r.chapters||[]).map(c=>[c.chapter,c]));
    failedIds=new Set((t.failures||[]).filter(f=>f.book===book.uuid).map(f=>f.id))};
  /* 「保存并刷新」这条 5 步链（flush 未落地的改字 → 重取整本书 → 重取同步状态 → 重画指定的几个
     子视图）原来在 triage/archiveEntry/restore/去处切换/转写/问 AI 七处各自逐字重复（2026-09-09
     审计发现），任何一处漏改都容易造成"某个动作之后画面没更新"这类不容易被发现的 bug——收成一个
     辅助函数，调用方只需要说清楚"这次要重画哪几个子视图"。 */
  /* 自己动作之后这里已经重取过整本书，同一动作让服务端发出的 `entries` 事件再触发一次整页刷新就是重复取（书列表 + 整本书 +
     同步状态 + 转写状态）：重取期间和之后 SELF_QUIET_MS 内到达的事件不再刷新（闸门 ②）。 */
  const reloadBook=async(...views)=>{gate.quietBegin();
    try{await flushPendingText();const b=await j(bookApi('ink'));if(b.ok!==false)book=b;await refreshSync();views.forEach(fn=>fn())}
    finally{gate.quietEnd()}};
  /* s 是章节级同步状态（notebookNeeded/Synced、obsidianNeeded/Synced），本身只精确到"整章"，不到
     "这一条"（`fingerprint_chapter` 把整章活条目内容拼一起算一个哈希，见白皮书 §03aa）。章头调用不传
     `only`，如实显示整章的聚合状态；贴在每条笔记行上时传 `only=该条自己的 destination`，把跟这条本身
     无关的那个去处的徽章过滤掉——不然一章里别的条目要笔记本，会让只选了 Obsidian 的那条也显示"笔记本
     未同步"，真机反馈"我只导了 Obsidian，实际显示两者都有"就是这个问题，见白皮书 §03ab。 */
  const syncBadges=(s,only)=>{if(!s)return'';
    const wantsNb=!only||only==='notebook'||only==='both',wantsOb=!only||only==='obsidian'||only==='both';
    // aria-label 跟 title 重复一份（2026-09-09 审计修）：图标本身 aria-hidden，屏幕阅读器原来只能
    // 念出 ✓/… 两个符号，丢了"这是关于设备笔记本/Obsidian"的语境。
    // `xxxNeeded` 是"现在这一刻还有没有条目要这个去处"，条目的 destination 一改就可能立刻翻成 false——
    // 徽章原来只看这个字段，导致真机 bug（2026-09-17 反馈）：一章先推过笔记本，再把（唯一）那条条目的
    // 去处切成 Obsidian，笔记本徽章直接消失，像是"刚推过的笔记本状态丢了"，但设备上的笔记本文档其实
    // 完好无损，只是网页不再显示它存在过。改成"现在要 或者 历史上推过"（`xxxGeneratedAt`/`xxxExportedAt`
    // 是 `notebooks.rs`/`export_state.rs` 记的历史事实，不会因为条目 destination 改了就清空）都显示，
    // ✓/… 仍然只看 `xxxSynced`（是否跟当前条目内容一致）——这样"曾经推过但现在没有条目要了"会诚实地
    // 显示成"…"（不是当前状态的镜像，见标题文案），而不是整个徽章凭空消失。
    const nb=wantsNb&&(s.notebookNeeded||s.notebookGeneratedAt!=null)?`<span class="badge${s.notebookSynced?' on':''}" title="${T('notes.sync.notebook',{state:s.notebookSynced?T('notes.sync.synced'):T('notes.sync.notebookPending')})}" aria-label="${T('notes.sync.notebook',{state:s.notebookSynced?T('notes.sync.synced'):T('notes.sync.notSynced')})}">📓${s.notebookSynced?'✓':'…'}</span>`:'';
    const ob=wantsOb&&(s.obsidianNeeded||s.obsidianExportedAt!=null)?`<span class="badge${s.obsidianSynced?' on':''}" title="${T('notes.sync.obsidian',{state:s.obsidianSynced?T('notes.sync.synced'):T('notes.sync.obsidianPending')})}" aria-label="${T('notes.sync.obsidian',{state:s.obsidianSynced?T('notes.sync.synced'):T('notes.sync.notSynced')})}">${OBSIDIAN_ICON}${s.obsidianSynced?'✓':'…'}</span>`:'';
    return nb+ob};
  const trashList=$('#ntrashlist',sec),trashSum=$('#ntrashsum',sec);
  const TRASH_STATUSES=['skipped','revoked','archived'];
  const trashedEntries=()=>(book.entries||[]).filter(e=>TRASH_STATUSES.includes(e.status));
  /* 回收站（点 3）：不是一个只会清空的黑盒按钮——列出「不需要」「不要了」「已撤销」的条目实际内容，
     清空前能看清要丢的是什么。数据不用额外接口：GET /books/{uuid} 本来就带全部条目（含终态的）。 */
  /* 「恢复」（回收站点 3）：Skipped/Revoked/Archived 都能恢复，落点由服务端按条目已有内容倒推
     （见 notecore::model::Entry::restore）——书里已经把笔画擦了也能恢复，找回的是条目库里已经存好
     的裁图/校对文本，不代表设备原页面的笔迹会重新出现（这条限制在页面文案里说清楚，不是网页能力）。 */
  const restoreOne=id=>entryAct(id,'restore');
  const renderTrash=()=>{if(!book){trashList.innerHTML=`<p class="small">${T('notes.pickBookFirst')}</p>`;trashSum.textContent='';return}trashList.innerHTML='';
    const items=trashedEntries().sort((a,b)=>b.updated-a.updated);
    trashSum.textContent=items.length?T('notes.trash.count',{count:items.length}):T('notes.trash.empty');
    if(!items.length){trashList.innerHTML=`<p class="small">${T('notes.trash.noneHint')}</p>`;return}
    items.forEach(e=>{const row=el('div',{class:'trash-item'});
      const text=e.text||(e.drafts&&e.drafts[0]&&e.drafts[0].text)||(e.quote&&e.quote.text)||T('notes.noTextContent');
      const dv=e.destination||'both';
      const chSync=e.chapter!=null?syncMap.get(e.chapter):null;
      // 这条本身去哪（配置的目的地）+ 它所在章节目前的生成/导出状态（章节维度，不是这条自己确认被
      // 收进去了没——归档/撤销后这条已经不在活条目集合里，没法再逆推"当初有没有被打进那次生成"，
      // 只能诚实地给"这一章大致是什么状态"这个参考信息，用户反馈"回收站该显示导出到哪里"）。
      row.innerHTML=`<div class="trash-badges"><span class="badge">${statusName(e)}</span><span class="badge">${DEST_ICON[dv]()}</span>${syncBadges(chSync,dv)}</div>
        <div class="txt">p.${e.page_index+1}${e.chapter_title?' · '+esc(e.chapter_title):''}<br><span class="q">${esc(text)}</span>${e.status==='revoked'?`<br><span class="small">${T('notes.trash.revokedHint')}</span>`:''}</div>
        <button class="btn" data-restore>${T('notes.trash.restoreBtn')}</button>`;
      guardClick(row.querySelector('[data-restore]'),async()=>{if(!(await restoreOne(e.id)))return;await reloadBook(renderTrash,renderBrowse,renderBook)});
      trashList.appendChild(row)})};
  /* 「导入 md 文档」：单篇 markdown → 一份新设备笔记本文档，独立于条目库（不经浏览/整理/回收站那条
     状态机，见 note-serve::publish::import_markdown）。用户明确要求别塞进「整理」——那边是审阅真被
     要求转笔记的条目，跟"拿一份现成 .md 文件直接生成一份新笔记"是两件不同的事，各自一个入口。
     2026-09-10 从"文本框打字"改成"选一个 .md 文件"：文件内容用 FileReader 在浏览器里读成字符串，
     继续走现有的 JSON POST（{title,markdown}）——后端 import_markdown() 本来就是吃一个纯字符串，
     用户角度"选/拖文件"和"打字"的体验差异已经达到了，没必要为了这层不可见的传输差异去碰
     note-serve 的路由/multipart 解析，多一层没必要的风险面。 */
  const importTitle=$('#nimporttitle',sec),importFile=$('#nimportfile',sec),importFilename=$('#nimportfilename',sec),importBtn=$('#nimportbtn',sec),importStat=$('#nimportstat',sec);
  let importFileContent='';
  const renderImport=()=>{const ready=!!book;importTitle.disabled=importFile.disabled=importBtn.disabled=!ready;
    importStat.textContent=ready?'':T('notes.pickBookFirstShort')};
  importFile.onchange=async()=>{
    const f=importFile.files[0];
    if(!f){importFileContent='';importFilename.textContent='';return}
    importFileContent=await f.text();
    importFilename.textContent=T('common.paren',{text:f.name,inner:fmtB(f.size)});
    if(!importTitle.value.trim())importTitle.value=f.name.replace(/\.(md|markdown)$/i,''); // 顺手拿文件名当默认标题，仍可编辑
  };
  importBtn.onclick=async()=>{if(!book)return;
    const title=importTitle.value.trim()||(importFile.files[0]?importFile.files[0].name.replace(/\.(md|markdown)$/i,''):'');
    const markdown=importFileContent;
    if(!title){toast(T('notes.import.needTitle'),'warn');return}
    if(!markdown.trim()){toast(T('notes.import.needFile'),'warn');return}
    importBtn.disabled=true;importStat.textContent=T('notes.import.generating');
    const r=await postJ(bookApi('notes',`/import-md`),{title,markdown}); // 失败时 postJ 已经弹过提示
    importBtn.disabled=false;
    if(r.ok===false){importStat.textContent='';return}
    importStat.textContent=T('notes.import.done',{name:r.visibleName});importFile.value='';importFileContent='';importFilename.textContent=''};
  guardClick($('#nrestoreall',sec),async()=>{if(!book)return;
    const items=trashedEntries();
    if(!items.length){toast(T('notes.trash.noneToRestore'),'warn');return}
    if(!await confirmDialog(T('notes.trash.confirmRestoreAll',{count:items.length})))return;
    for(const e of items)await restoreOne(e.id);
    await reloadBook(renderTrash,renderBrowse,renderBook)});
  /* 浏览态动作：Mined→Pending（转入笔记）/ Mined→Skipped（不需要），见 ink-serve::triage。三个子视图都要重画（条目跨视图搬家）。 */
  const triage=async(id,action)=>{if(await entryAct(id,action))await reloadBook(renderBrowse,renderBook,renderTrash)};
  const updateSummary=()=>{if(!book){sum.textContent='';return}const es=book.entries||[];
    const c=st=>es.filter(e=>e.status===st).length;
    sum.textContent=T('notes.summary',{mined:c('mined'),pending:c('pending'),draft:c('draft'),reviewed:c('reviewed')})};
  // 全站唯一一处"按钮文案暗示有代价、却没有二次确认"（2026-09-09 审计发现）：清掉页记录会强制整本
  // 重新摄取。实际数据风险不大（已校对文本/条目不会被覆盖，见 notecore::ingest 的增量规则），但操作
  // 本身不常用、容易误触，补一句说清楚"安全在哪"的确认。
  guardClick($('#nrescan',sec),async()=>{if(!book)return;if(!await confirmDialog(T('notes.confirmRescan')))return;await flushPendingText();await postJ(bookApi('ink',`/rescan`),{});refresh()});
  guardClick($('#npurge',sec),async()=>{if(!book)return;
    const items=trashedEntries();
    if(!items.length){toast(T('notes.trash.noneToPurge'),'warn');return}
    if(!await confirmDialog(T('notes.trash.confirmPurge',{count:items.length})))return;
    await flushPendingText();
    if((await sendT(bookApi('ink',`/purge`),'POST',undefined,'notes.trash.purgeFailed')).ok===false)return;
    // 书列表（条目数）和整本书都变了：走一次完整刷新（不受结果提示的暂停挡）。此前直接 `book=await j(…)`，取失败时
    // `book` 变成 {ok:false}，之后的请求会拼出 /books/undefined，直到下一次刷新才恢复。
    await runRefresh(true,false,true)});
  /* 「不要了」（三期）：转 Archived，两处投影都摘掉，条目库里软删留痕（真删靠「回收站」清空）。 */
  const archiveEntry=async(id)=>{if(!await confirmDialog(T('notes.confirmArchive')))return;
    if(await entryAct(id,'archive'))await reloadBook(renderBook,renderTrash)};
  /* 浏览：按页分组、只列 Mined（待决定的），最近变更的页在前；转入笔记/不需要两个按钮直接调 triage。 */
  const renderBrowse=()=>{if(!book){browse.innerHTML=`<div class="card"><p class="small">${T('notes.pickBookFirst')}</p></div>`;return}browse.innerHTML='';updateSummary();
    const mined=(book.entries||[]).filter(e=>e.status==='mined');
    if(!mined.length){browse.innerHTML=`<div class="card"><p class="small">${T('notes.browse.empty')}</p></div>`;return}
    const groups=new Map();mined.forEach(e=>{const k=e.page_index;if(!groups.has(k))groups.set(k,[]);groups.get(k).push(e)});
    const recency=k=>Math.max(...groups.get(k).map(e=>e.updated));
    [...groups.keys()].sort((a,b)=>recency(b)-recency(a)).forEach(k=>{const es=groups.get(k).sort((a,b)=>(a.ink?a.ink.bbox[1]:0)-(b.ink?b.ink.bbox[1]:0));
      const card=el('div',{class:'card'});card.innerHTML=`<h3 style="margin-top:0">${T('notes.pageHeading',{page:k+1})}${es[0].chapter_title?' · '+esc(es[0].chapter_title):''} <span class="small">${T('notes.entryCount',{count:es.length})}</span></h3>`;
      es.forEach(e=>{const row=el('div',{class:'entry'});
        row.innerHTML=`<div class="entry-body">
          <div class="entry-crop">${cropHtml(e)}</div>
          <div class="entry-main">
            ${e.quote?`<div class="entry-quote">「${esc(e.quote.text)}」</div>`:''}
            <div class="entry-ops"><div class="grp"><button class="btn pri" data-a="request">${T('notes.browse.request')}</button><button class="btn" data-a="skip">${T('notes.browse.skip')}</button></div></div>
          </div></div>`;
        row.querySelector('[data-a="request"]').onclick=()=>triage(e.id,'request');
        row.querySelector('[data-a="skip"]').onclick=()=>triage(e.id,'skip');
        card.appendChild(row)});
      browse.appendChild(card)})};
  /* 整理：只列真被要求转笔记的（Pending/Draft/Reviewed）——Mined 在「浏览」决定，Skipped/Revoked/Archived 去「回收站」。
     每条卡片三块视觉分区（点 4）：左手写裁图 ｜ 中转写/校对文本 ｜ 下问 AI 区，宽屏并排、手机堆叠（style.css .entry-*）。
     **第二轮反馈（2026-09-08）改动**：① 样式不再是下拉——`text` 一存，服务端就按行首 `-`/`1.`/`口`/`##`
     标记自动判样式（`notecore::model::Entry::apply_marked_text`），这里只显示一个只读徽章。② 去处
     （设备笔记本/Obsidian/都要）从下拉换成紧凑图标循环按钮，点一下切下一态。③ 每条一个勾选框；④ 转写
     失败的条目标红、按钮文案变「重转失败」——失败清单查一次 `/api/transcribe/status` 按条目 id 对上。
     **第三轮反馈（2026-09-08）改动**：生成笔记本/导出 md 挪回章头直接按钮（不再要求先勾选——这两个
     操作本来就是整章一起投影，选中哪几条对结果没有过滤作用，硬要求先勾选只是绕远路），批量勾选工具栏
     收窄成只剩真正逐条起作用的重转/不要了；章头新增 📓/Obsidian 同步徽章，全同步的章节默认从列表收起
     （"生成完成后是不是应该移出列表"），有「显示已同步的章节」开关能翻出来；回收站每条显示去处徽章 +
     所在章节的同步状态（"回收站该显示导出到哪里"），见 `refreshSync()`/`syncBadges()`/白皮书 §03x。
     **第四轮反馈（同一天）**：生成笔记本/导出 md 这两个按钮又被指出跟条目已有的「去处」字段重复——
     去处早就决定了这一章该不该落笔记本、该不该落 Obsidian，合并成一个按钮，内部按去处该做哪样做哪样，
     不用用户自己对着两个按钮再选一遍"点哪个"，见白皮书 §03y。
     **第五轮反馈（同一天）**：与其用一个「显示已同步的章节」复选框过滤一条长列表，改成「未导出/已导出」
     两个顶层 tab（复用 `fullySynced` 判据分组，两者互斥且穷尽，没有第三态），tab 下面章节做成第二层
     可点标签，点哪章只显示哪一章的内容——比上一轮"章节默认折叠"更彻底：折叠只是不用看见内容，滚动
     那条轴还在；这样任意时刻屏幕上最多一章的内容，滚动本身消失。旧的 `expandedChapters`（折叠/展开）
     整个被 `exportTab`/`selectedChapter` 两个状态取代。同步状态目前只精确到整章，做不到"这条笔记本身
     导出过没"，退而求其次把章节级同步徽章也贴一份到每条笔记行上。顺带把「同步本章」改名「推送本章」
     （"同步"暗示双向，这个按钮其实只单向推）、「重转」改名「重新转写」（跟「浏览」视图里语义完全不同
     的「转入笔记」共享"转"字，容易混），见白皮书 §03z。
     **第七轮反馈（同一天）**：① 批量勾选（每条复选框/「全选本章」/顶部隐藏工具栏）整段删除——那两个
     操作（重新转写/不要了）现在每条自己就有独立按钮，勾选层是纯粹的重复入口，跟这条线一贯"有独立
     按钮就别再叠一层批量选择"的取舍一致。② tab 归属判据从"整章内容是否跟最近一次投影完全匹配"
     （`fullySynced`）改成"这一章有没有被推送过"（`everExported`，只看 `notebookGeneratedAt`/
     `obsidianExportedAt` 是否非空）——原判据是两个布尔值的组合，编辑任意一条笔记的内容/落点都可能
     让整章的指纹对不上，"已导出"章节因为一次小编辑弹回"未导出"，用户反馈"多条数据的组合判断，一
     改落点就变成未导出"。改判存在性之后，推送过一次就稳定留在「已导出」，不再随内容变化在两个 tab
     间跳；`fullySynced`/`syncBadges` 没有被替换掉——它们继续管"章头/每条笔记的 ✓/… 徽章"，"这一章
     还有没有新改动没推送"这条信息没有丢，只是从"决定进哪个 tab"降级成"已导出 tab 内的一个提示"，
     见白皮书 §03ad。 */
  const exportTabsEl=$('#nexporttabs',sec);
  /* 双层导出状态视图（第五轮反馈，用户反馈"1章10条笔记，10章就100条，手机划几分钟才到底"）：
     顶层「未导出/已导出」两个 tab（第七轮反馈改成按 everExported 分组，见上），tab 下再按章节列第
     二层可点标签，点哪章只显示哪一章的内容——任意时刻屏幕上最多一章的内容，不靠折叠/滚动去缓解。
     exportTab 跨换书保留（比照原复选框状态本来也不随换书重置），selectedChapter 换书清空（章节 key
     按书算）。 */
  let exportTab='pending',selectedChapter=null;
  const renderBook=async(opts={})=>{if(!book){chaptertabs.innerHTML='';chapterbody.innerHTML=`<p class="small">${T('notes.pickBookFirst')}</p>`;return}updateSummary();
    const advance=!!opts.advance;
    // "活条目"（进投影、在「整理」里列出）由 ink-serve 在每个条目上给出 `live`
    // （= notecore Status::is_live_for_projection()，2026-10-10 起；此前这里手抄一份状态名单）。
    const live=(book.entries||[]).filter(e=>e.live);
    const groups=new Map();live.forEach(e=>{const k=e.chapter==null?-1:e.chapter;if(!groups.has(k))groups.set(k,[]);groups.get(k).push(e)});
    const sortedKeys=[...groups.keys()].sort((a,b)=>a-b);
    // fullySynced：整章内容是否跟最近一次投影完全匹配——只用来算"✓/…"徽章，不再决定 tab 归属
    // （第七轮反馈：这两件事拆开，见上面大注释）。everExported：这一章有没有被推送过至少一次，
    // 决定 tab 归属，推送过就稳定留在「已导出」，不会因为后续编辑内容/落点又弹回「未导出」。
    const fullySynced=k=>{const s=k>=0?syncMap.get(k):null;return !!(s&&s.notebookSynced&&s.obsidianSynced)};
    const everExported=k=>{const s=k>=0?syncMap.get(k):null;return !!(s&&(s.notebookGeneratedAt!=null||s.obsidianExportedAt!=null))};
    const pendingKeys=sortedKeys.filter(k=>!everExported(k));
    const syncedKeys=sortedKeys.filter(k=>everExported(k));
    /* 选中章节的归属判定：默认"跟随"——只要这一章还有活条目，不管它现在算未导出还是已导出，都继续
       显示它，只是把 tab 高亮切到它现在所在的那边（真机反馈：点了条目自己的去处按钮后画面跳到了别
       的章节，读起来像数据错乱——其实是没有跟随，被"选中章节必须在当前 tab 可见列表里"这条校验当成
       "消失"处理了，随手选中了列表里第一个不相干的章节）。只有两种情况允许真的换到别的章节：显式点
       了顶层 tab 按钮（点击处理器会先把 selectedChapter 置空，走下面的兜底分支）、或显式要求"推送完
       这章就跳下一个待处理的"（`advance`，只有「推送本章」成功后传 true，是那个按钮特有的"处理完
       继续下一条"工作流，不该套用到编辑动作上）。 */
    if(selectedChapter!=null&&groups.has(selectedChapter)&&!advance){
      exportTab=everExported(selectedChapter)?'synced':'pending';
    }else{
      const pick=exportTab==='pending'?pendingKeys:syncedKeys;
      selectedChapter=pick.length?pick[0]:null;
    }
    exportTabsEl.querySelectorAll('button').forEach(b=>b.classList.toggle('on',b.dataset.etab===exportTab));
    const visibleKeys=exportTab==='pending'?pendingKeys:syncedKeys;
    chaptertabs.innerHTML='';
    visibleKeys.forEach(k=>chaptertabs.appendChild(btn(k<0?T('notes.unfiledChapter'):T('notes.chapterHeading',{n:k+1}),()=>{selectedChapter=k;renderBook()},k===selectedChapter?'on':'')));
    chapterbody.innerHTML='';
    if(selectedChapter==null){
      // 2026-09-09 审计修：pendingKeys 为空有两种完全不同的原因——"这本书压根没有条目被转入笔记过"
      // （sortedKeys 本身是空的）vs"都推送完了"（sortedKeys 非空但全部落进了已导出）。原文案不分这
      // 两种情况一律显示庆祝 emoji，容易让"还什么都没做"的用户误以为自己已经完成了操作。
      const emptyMsg=exportTab==='pending'
        ?(sortedKeys.length?T('notes.export.allPushed'):T('notes.export.noEntries'))
        :T('notes.export.nonePushedYet');
      chapterbody.innerHTML=`<p class="small">${emptyMsg}</p>`;
      return}
    const k=selectedChapter,es=groups.get(k).sort((a,b)=>a.page_index-b.page_index||(a.ink?a.ink.bbox[1]:0)-(b.ink?b.ink.bbox[1]:0));
    const s=k>=0?syncMap.get(k):null;
    const card=el('div',{class:'card'});
    card.innerHTML=`<h3 style="margin-top:0">${k<0?T('notes.unfiledChapterParen'):esc(T('notes.chapterHeadingTitled',{n:k+1,title:es[0].chapter_title||''}))} <span class="small">${T('notes.entryCount',{count:es.length})}</span></h3>${k>=0?`<div class="row"><button class="btn pri" data-sync title="${T('notes.pushChapterTitle')}">${T('notes.pushChapterBtn')}</button>${syncBadges(s)}<span class="small" data-genmsg></span></div>`:''}<div data-body></div>`;
    const body=card.querySelector('[data-body]');
    if(k>=0){
      const syncBtn=card.querySelector('[data-sync]'),msg=card.querySelector('[data-genmsg]'),row=card.querySelector('.row');
      // 直接章头按钮，不用先勾选条目——生成/导出本来就是整章一起投影（条目挑不挑没用，见白皮书
      // §03x"是不是重复了"）；批量勾选整层第七轮反馈已经整段删掉，重新转写/不要了现在各自逐条一个
      // 独立按钮，见上面模块注释。
      // **合并成一个按钮（第四轮反馈）、改名「推送本章」（第五轮反馈）**：去处（Entry.destination）
      // 本来就已经决定了这一章该不该生成笔记本、该不该导出 md——分两个按钮让用户自己再选一遍"点哪个"
      // 是重复劳动，一个按钮内部按当前去处该做哪样做哪样：没有条目要那个去处，对应那步自然是 Empty
      // （后端已有这个语义，见 export::ExportOutcome/publish::ChapterOutcome），前端只是不重复提示
      // "没做"；改名"推送"是因为"同步"暗示双向/拉取，这个按钮其实只单向推。
      syncBtn.onclick=async()=>{syncBtn.disabled=true;msg.textContent='';gate.hold(NOTE_GATE.HOLD_BUSY_MS,'busy');
        // 服务端没有天然的分步数据（耗时来自生成笔记本+导出 md 两次整章调用，不是可数的"第几步"）——
        // 跟母版库普通整本落库同一处境，共用同一套不确定态滚动条（2026-09-19 代码质量审计，
        // 原来这里只有一句不会变的静态文字"推送中…"）。
        const prog=renderBusy(row,T('notes.pushing'));
        const gr=await j(bookApi('notes',`/chapters/${k}/generate`),{method:'POST'});
        const er=await j(bookApi('notes',`/chapters/${k}/export`),{method:'POST'});
        prog.remove();
        syncBtn.disabled=false;
        const gc=(gr.chapters&&gr.chapters[0])||{};
        const parts=[];
        const fail=(label,m)=>parts.push('✗ '+T('common.labelValue',{label,value:m}));
        if(gr.ok===false)fail(T('notes.push.notebook'),gr.message||T('common.failed'));
        else if(gc.status==='failed')fail(T('notes.push.notebook'),gc.message);
        else if(gc.status==='generated')parts.push('✓ '+T('notes.push.notebookUpdated'));
        if(er.ok===false)fail('md',er.message||T('common.failed'));
        // md 用一条带链接的提示让用户点开：此前在两次请求之后才 window.open，已经不在点击的上下文里，iOS Safari / Firefox
        // 当弹窗拦掉，Chrome 操作超过约 5 秒也拦——设备上生成笔记本常要好几秒，md 实际打不开。
        else if(er.status==='written'){parts.push('✓ '+T('notes.push.mdExported'));toastLink(T('notes.push.mdExported'),T('notes.push.openMd'),bookApi('notes',`/chapters/${k}/export.md`))}
        msg.textContent=parts.length?parts.join(' · '):T('notes.push.noChange');
        // 结果文案刚显示出来，从这一刻起再保 3s，不管上面两次请求实际花了多久
        await lingerThen(async()=>{await refreshSync();renderBook({advance:true})})};
    }
    es.forEach(e=>{const failed=failedIds.has(e.id);const row=el('div',{class:'entry'+(failed?' entry-failed':'')});
      const draft=(e.drafts&&e.drafts[0])?e.drafts[0].text:'';
      const dv=e.destination||'both';
      row.innerHTML=`
        <div class="entry-head">
          <span>p.${e.page_index+1}${e.subhead?' · '+esc(e.subhead):''}</span>
          <span class="badge">${T(STYLE_NAMES[e.style])||esc(e.style)}</span>
          ${syncBadges(s,dv)}
          <span class="badge ${e.status==='reviewed'?'on':''}" style="margin-left:auto">${statusName(e)}</span>
        </div>
        <div class="entry-body">
          <div class="entry-crop">${cropHtml(e)}</div>
          <div class="entry-main">
            ${e.quote?`<div class="entry-quote">「${esc(e.quote.text)}」</div>`:''}
            <textarea class="entry-text" rows="2" placeholder="${esc(draft?T('notes.draftPlaceholder',{draft}):T('notes.waitingTranscribe'))}">${esc(e.text||draft)}</textarea>
            <div class="small">${T('notes.styleHint')}</div>
            <div class="entry-ops">
              <div class="grp"><button class="btn" data-dest title="${T('notes.dest.switchTitle')}">${DEST_ICON[dv]()} <span aria-hidden="true" style="opacity:.55">⟳</span></button></div>
              <div class="grp">${(e.ink&&e.ink.crop)?`<button class="btn${failed?' btn-bad':''}" data-transcribe title="${T('notes.retranscribeTitle')}">${failed?T('notes.transcribeFailed'):(draft?T('notes.retranscribe'):T('notes.transcribe'))}</button>`:''}<button class="btn" data-archive title="${T('notes.archiveTitle')}">${T('notes.archiveBtn')}</button></div>
            </div>
            <div class="small" data-txstat></div>
            <div class="entry-ask">
              <div class="row"><label class="toggle"><input type="checkbox" data-ask ${e.ask_ai?'checked':''}> ${T('notes.askAi')}</label>
                <input type="text" data-question placeholder="${T('notes.questionPlaceholder')}" value="${e.question?esc(e.question):''}" style="flex:1;min-width:9em" ${e.ask_ai?'':'disabled'}>
                <button class="btn pri" data-askbtn ${e.ask_ai&&e.question?'':'disabled'}>${T('notes.askBtn')}</button></div>
              <div class="small" data-askstat></div>
              ${e.answer?`<div class="entry-answer">${T('common.paren',{text:`<b>${T('notes.aiAnswer')}</b>`,inner:esc(T('notes.askedLabel',{brief:e.answer.brief}))})}<br>${esc(e.answer.text).replace(/\n/g,'<br>')}</div>`:''}
            </div>
          </div>
        </div>`;
      const ta=row.querySelector('textarea');
      ta.oninput=ev=>pendingText.set(e.id,ev.target.value);   // 还没失焦确认，先记住最新值，别的动作重画前会先冲掉
      // 存上了才从 pendingText 里摘掉（存失败的那份留着，下次重画前 flushPendingText 再试；期间又改过就不摘新值）。
      ta.onchange=ev=>{const v=ev.target.value;patch(e.id,{text:v}).then(ok=>{if(ok&&pendingText.get(e.id)===v)pendingText.delete(e.id)})};
      guardClick(row.querySelector('[data-dest]'),async()=>{const next=DEST_ORDER[(DEST_ORDER.indexOf(dv)+1)%3];await patch(e.id,{destination:next});await reloadBook(renderBook)});
      row.querySelector('[data-archive]').onclick=()=>archiveEntry(e.id);
      /* 点「重新转写」/「提问」弹出状态和这次调用的消耗（用户反馈"应该弹出状态及当前消耗"，2026-09-08
         第三轮）：先显文字状态（转写中…/提问中…），拿到结果显示"✓ 完成 · token 入X 出Y"或错误，
         停留一小会儿让用户真的看得到（不然紧接着的整页重画会立刻把这条状态盖掉，等于白显示）——
         最初给的 1.5s 真机反馈"闪一下就没了"根本来不及读，2026-09-16 延长到 3s（「推送本章」
         那条同款状态提示也一起延长，三处是同一个模式）。 */
      const tb=row.querySelector('[data-transcribe]'),txStat=row.querySelector('[data-txstat]');
      // 转写失败也重取：失败清单（failedIds）变了，这条要标红、按钮改「重转失败」。
      if(tb)tb.onclick=()=>callModel(tb,txStat,entryApi('transcribe',e.id),'notes.transcribing','notes.transcribeFailed','notes.transcribeDone',true);
      /* 「问AI」勾选框 + 问题 + 提问按钮：改即存（ink-serve），点提问才真的调 mind-serve。 */
      const askBox=row.querySelector('[data-ask]'),qInput=row.querySelector('[data-question]'),askBtn=row.querySelector('[data-askbtn]'),askStat=row.querySelector('[data-askstat]');
      const syncAskUi=()=>{qInput.disabled=!askBox.checked;askBtn.disabled=!(askBox.checked&&qInput.value.trim())};
      askBox.onchange=()=>{patch(e.id,{askAi:askBox.checked});syncAskUi()};
      qInput.onchange=()=>{patch(e.id,{question:qInput.value});syncAskUi()};
      askBtn.onclick=()=>callModel(askBtn,askStat,entryApi('mind',e.id,'/ask'),'notes.asking','notes.askFailed','notes.askDone',false);
      body.appendChild(row)});
    chapterbody.appendChild(card)};
  exportTabsEl.querySelectorAll('button').forEach(b=>b.onclick=()=>{if(exportTab===b.dataset.etab)return;exportTab=b.dataset.etab;selectedChapter=null;renderBook()});
  /* 取当前选中的书并重画。先取到局部变量再一次性换掉 `book`：此前开头先置空，等待期间任何会调 bookApi() 的动作（文本框
     失焦保存等）都会因 book 为 null 抛错、这次编辑丢掉。选中的章节在这里不清空（只有换书才清，见 sel.onchange）——
     事件刷新也走这里，此前每次保存引发的 `entries` 事件都把用户正在看的那章跳回第一章。 */
  const loadBook=async()=>{await flushPendingText();
    let b=null;
    if(sel.value){const r=await j(`/api/ink/books/${encodeURIComponent(sel.value)}`);
      // 取失败（ink-serve 重启中、书刚被删）按"没选书"画，别把 {ok:false} 当成书——此前后续请求会拼出 /books/undefined。
      if(r.ok===false)toast(r.message||T('common.failed'));else b=r}
    if(!b||!book||b.uuid!==book.uuid)selectedChapter=null; // 换了书（或书没了），章节 key 按书算，清掉
    book=b;
    await refreshSync();renderBrowse();await renderBook();renderTrash();renderImport()};
  sel.onchange=()=>runRefresh(false,false,true);
  /* 全文搜索（跨所有书，ink-serve /search）：结果点一下就切到那本书的「浏览」。命中词加粗——片段先 esc 再替换，
     替换用的也是 esc 过的查询词，不会引入未转义的 HTML。 */
  const nq=$('#nq',sec),nqres=$('#nqres',sec);
  const FIELD_KEYS={quote:'notes.search.field.quote',text:'notes.search.field.text',draft:'notes.search.field.draft',question:'notes.search.field.question',answer:'notes.search.field.answer',title:'notes.search.field.title'};
  const mark=(snip,q)=>{const e=esc(snip),qe=esc(q);if(!qe)return e;const i=e.toLowerCase().indexOf(qe.toLowerCase());return i<0?e:e.slice(0,i)+'<b>'+e.slice(i,i+qe.length)+'</b>'+e.slice(i+qe.length)};
  const doSearch=async()=>{const q=nq.value.trim();nqres.innerHTML='';if(!q)return;
    const r=await j('/api/ink/search?q='+encodeURIComponent(q));
    if(r.ok===false){nqres.innerHTML=`<p class="small">${esc(r.message||T('common.failed'))}</p>`;return}
    const items=r.items||[];
    if(!items.length){nqres.innerHTML=`<p class="small">${T('notes.search.none')}</p>`;return}
    const ul=el('ul',{class:'nsearch'});
    items.forEach(h=>{const li=el('li',{html:`<div class="small">${esc(h.title)} · p.${h.pageIndex+1}${h.chapterTitle?' · '+esc(h.chapterTitle):''} · ${T(FIELD_KEYS[h.field]||'notes.search.field.text')}</div><div>${mark(h.snippet,q)}</div>`});
      // 跳到这条目实际所在的子页：未处理（mined）在「浏览」，跳过/撤销/归档在「回收站」（与回收站列表同一份 TRASH_STATUSES；
      // 此前漏了 revoked，已撤销的命中会跳到「整理」、那里根本不列它），其余（待转写/草稿/定稿）在「整理」。
      const tab=h.status==='mined'?0:TRASH_STATUSES.includes(h.status)?2:1;
      li.onclick=async()=>{if(sel.value!==h.uuid){sel.value=h.uuid;await runRefresh(false,false,true)}$('#nsubnav',sec).children[tab].click();nqres.innerHTML=''};
      ul.appendChild(li)});
    nqres.appendChild(el('p',{class:'small',text:T('notes.search.count',{n:items.length})}));nqres.appendChild(ul)};
  guardClick($('#nqgo',sec),doSearch);
  nq.addEventListener('keydown',e=>{if(e.key==='Enter')doSearch()});
  /* 「导入 md 文档」子标签的显示/隐藏跟着「管理→实验室」的 notesImportMdEnabled 开关走——用
     hidden 属性而不是从 DOM 移除（subtabs() 是纯位置下标配对，移除会让后面的子标签全部错位）。
     没有 SSE 推送这个开关的变化，靠 sec.refresh（笔记 tab 每次从别的 tab 切回来都会调，见文件
     末尾 IIFE 里 addTab 的点击处理）顺带每次重新拉一次状态，跟这个 app 里"tab 记脏、切回时刷新"
     的既有设计一致。 */
  const importNavBtn=$('#nsubnav',sec).children[3],importPanel=$('#nimport',sec);
  const syncImportVisible=async()=>{
    const r=await j('/api/enhance/status');
    const show=r.ok!==false&&!!r.notesImportMdEnabled;
    if(!show&&importNavBtn.classList.contains('on'))$('#nsubnav',sec).children[0].click(); // 正停在「导入」时先切回「浏览」，避免 hidden+on 类同时存在
    importNavBtn.hidden=!show;importPanel.hidden=!show;
  };
  /* 全部刷新入口走同一个 coalesce（切 tab、事件、换书、搜索跳转、清空回收站…）：此前切 tab 和事件各有一个合并器，
     两份 loadBook 可以并发跑。`checkImport`：只有切回本 tab 的刷新重查「导入 md」开关（那个开关在「管理」页改）——
     自动转写期间每转完一条都有事件，原来每次都连带让网关扫一遍 xochitl 扩展状态。 */
  const doRefresh=async()=>{const want=gate.take();
    if(!want)return; // 正显示着结果提示，别被 SSE 抢跑冲掉（闸门 ①）；用户自己换书是 force，不挡
    const {checkImport,list}=want;
    if(list){const d=await j('/api/ink/books');const cur=sel.value;sel.innerHTML=(d.items||[]).map(b=>`<option value="${esc(b.uuid)}">${esc(T('common.paren',{text:b.title,inner:b.entries}))}</option>`).join('')||`<option value="">${T('notes.noBooks')}</option>`;
      if(cur&&[...sel.options].some(o=>o.value===cur))sel.value=cur}
    await loadBook();if(checkImport)await syncImportVisible()};
  const runCoalesced=coalesce(doRefresh);
  const runRefresh=(list=true,checkImport=false,force=false)=>{gate.request(list,checkImport,force);return runCoalesced()};
  const refresh=()=>runRefresh(true,true);
  /* 只有同步状态变了（note-serve 生成笔记本/导出 md 发 `notebooks`）：只重取同步状态、重画「整理」和回收站，不重取书列表和整本书。 */
  const syncOnly=coalesce(async()=>{if(gate.held()||!book)return;await refreshSync();await renderBook();renderTrash()});
  /* 事件刷新会整段重画（含正在编辑的文本框）：用户正在本 tab 的输入框里打字时先不重画，只记一笔，焦点离开输入框再补一次。
     此前自动转写/另一台设备的改动一来，光标连同输入框一起被重画掉（文字靠 pendingText 保住了，但得重新点进去）。 */
  const editing=()=>{const a=document.activeElement;return !!a&&sec.contains(a)&&(a.tagName==='TEXTAREA'||(a.tagName==='INPUT'&&/^(text|search)$/.test(a.type)))};
  sec.onEvent=ev=>{const act=gate.event(ev.kind,editing());
    if(act==='sync')syncOnly();else if(act==='refresh')runRefresh()}; // 'defer'/'quiet'：不刷（见 refreshGate）
  sec.addEventListener('focusout',()=>setTimeout(()=>{if(gate.blurred(editing()))runRefresh()},0));
  refresh();sec.refresh=refresh;subtabs(sec)}
