/* 母版库一本书是不是"已经不用管了"——给「已加入」筛选用：加入过 xochitl、不在处理中、最近一次加入没失败。
   正在处理/失败态都不算"完成"，归进「未加入」（还需要用户看见）。以前"加入过 KOReader"的书（2026-09-29 设备卸掉
   KOReader）不算完成。 */
const isBookDone=it=>{
  if(it.busy)return false;
  const dv=it.delivered||{};
  if(dv.deliver&&dv.deliver.status==='failed')return false;
  return !!dv.native;
};

/* 母版库怎么用：三步走 + 收哪些格式 + 加入 xochitl 适合什么书（2026-09-29 起设备只剩 xochitl 一个阅读器，
   原来"两读器怎么选/拿不准放哪"两条随 KOReader 一起撤掉） */
const GUIDE=()=>`<details class="cmp"><summary>${T('transfer.guide.summary')}</summary>
<dl class="help">
<dt>${T('transfer.guide.steps.dt')}</dt><dd>${T('transfer.guide.steps.dd')}</dd>
<dt>${T('transfer.guide.format.dt')}</dt><dd>${T('transfer.guide.format.dd',{native:up(EXT.book)})}</dd>
<dt>${T('transfer.guide.native.dt')}</dt><dd>${T('transfer.guide.native.dd')}</dd>
</dl></details>`;

/* 母版库列表（2026-09-20 重设计，兼顾手机和 PC）。每行 = 勾选框 + 清爽书名（去掉 `-- 作者 -- hash` 尾巴，
   完整名在 title 里）+ 一行徽章 + 状态/进度；行内没有按钮，操作（加入 xochitl / 下载 / 改名 / 删除）一律在勾选后的
   底部操作栏（用户 2026-09-20 定，别加回单条按钮）。批量走服务端队列
   （网关 `/api/batch`），关掉页面照跑。 */
const stgClean=n=>{const s=n.replace(/\.(epub|pdf)$/i,'');return (s.split(' -- ')[0]||s).trim()};
/* 搜索框的下拉建议：**书名 = 第一个 "-" 之前的内容**（用户 2026-09-20 指定）。"亂馬1⁄2 典藏版 - 07卷" → "亂馬1⁄2 典藏版"，
   同一本书的多卷合成一条；选中后按名字包含匹配，正好筛出这本书的所有卷。 */
const stgTitle=n=>stgClean(n).split('-')[0].trim();
const stgNameOptions=items=>[...new Set(items.map(it=>stgTitle(it.name)).filter(Boolean))].map(n=>`<option value="${esc(n)}">`).join('');
/* 「未加入」＝「已加入」的补集：还没加入过，或正在处理、上次加入失败（这些还需要用户看见，默认就停在这个筛选上）。
   两个筛选互补，数量加起来等于「全部」。 */
const stgIsTodo=it=>!isBookDone(it);
/* 一本书的徽章 HTML + 一条可见的状态文字（失败原因等）：格式/大小/落库记录/渲染自检/忙态。 */
function stgBadges(it,busy){
  const fmt=bookFmtLabel(it);
  const dv=it.delivered||{},stale=t=>t&&it.mtime&&t<it.mtime;
  // 只标「已加入 xochitl」：落库记录里历史上的 `koreader` 那条不再显示（2026-09-29 设备已卸载 KOReader）。
  const dl=dv.native?`<span class="badge on" title="${stale(dv.native)?T('transfer.staging.delivered.native.staleTitle'):T('transfer.staging.delivered.native.title')}">${T('transfer.staging.delivered.native.badge')}${stale(dv.native)?T('transfer.staging.staleSuffix'):''}</span>`:'';
  // 渲染自检只记页数（2026-10-07 起不再按字数估期望页数报"只渲染了 N 页"；旧记录里的 warn 也按页数显示）。
  const rc=dv.render,rb=!rc?'':rc.status==='onopen'?`<span class="badge" title="${T('stg.render.onopenTitle')}">${T('stg.render.onopenBadge')}</span>`:rc.status==='ok'||rc.status==='warn'?`<span class="badge on" title="${T('stg.render.okTitle',{pages:rc.pages})}">${T('transfer.staging.render.okBadge',{pages:rc.pages})}</span>`:rc.status==='pending'?`<span class="badge" title="${T('transfer.staging.render.pendingTitle')}">${T('transfer.staging.render.pendingBadge')}</span>`:`<span class="badge" title="${T('transfer.staging.render.noneTitle')}">${T('transfer.staging.render.noneBadge')}</span>`;
  const dc=dv.deliver;
  // 卡在 pending 但 busy=false＝上次处理被服务/设备重启打断（2026-09-19 真机撞过），不是"还在跑"。
  const stalePending=dc&&dc.status==='pending'&&!it.busy;
  const fails=(stalePending?`<span class="badge off" title="${T('transfer.staging.stalePending.title')}">${T('transfer.staging.stalePending.badge')}</span>`:'')
    +(dc&&dc.status==='failed'?`<span class="badge off" title="${esc(dc.message)}">${T('transfer.staging.deliverFailed.badge')}</span>`:'');
  const msg=dc&&dc.status==='failed'?T('transfer.staging.deliverFailedPrefix')+dc.message:stalePending?T('transfer.staging.stalePending.title'):'';
  return {html:`<span class="badge fmt">${esc(fmt)}</span><span class="stg-size">${fmtB(it.bytes)}</span>${dl}${rb}${busy?'':fails}`,msg};
}
/* 一行。ctx: {picked,batchQueued,bs,syncSel()} */
function stgRow(it,ctx){
  const dc=(it.delivered||{}).deliver;
  const busy=!!it.busy;
  const queued=ctx.batchQueued.has(it.name)||(ctx.bs.running&&ctx.bs.current===it.name&&!busy);
  const b=stgBadges(it,busy);
  const cb=el('input',{type:'checkbox','aria-label':it.name});cb.checked=ctx.picked.has(it.name);
  cb.onchange=()=>{if(cb.checked)ctx.picked.add(it.name);else ctx.picked.delete(it.name);li.classList.toggle('sel',cb.checked);ctx.syncSel()};
  const title=el('div',{class:'stg-name',title:it.name,text:stgClean(it.name)});
  const meta=el('div',{class:'stg-meta',html:b.html});
  const main=el('div',{class:'stg-main'},[title,meta]);
  if(b.msg)main.appendChild(el('div',{class:'small stg-err',text:b.msg}));
  if(busy)renderBusy(main,dc&&dc.status==='pending'?T('transfer.staging.progress.delivering'):T('transfer.staging.progress.working'));
  else if(queued)renderBusy(main,T('stg.batch.queuedHere'));
  // 列表只显示书名/类型/大小/状态/进度；**所有操作**（加入 xochitl/删除/全部中止）由勾选后的底部操作栏统一控制
  // （用户 2026-09-20 明确要求）。行内原来唯一的按钮是并发闸门排队时的「取消排队」，闸门 2026-10-07 删掉后行内没有按钮。
  const li=el('li',{class:'stg-row'+(cb.checked?' sel':'')},[el('label',{class:'stg-check'},[cb]),main]);
  return li;
}

/* 「传书」固定 tab = 入库（上传 / scp 进 inbox）｜母版库（加入 xochitl）。放第一位。书架不优化书：书先在电脑上用
   sheng-ren 的 xochitl 阅读模式优化好再上传（2026-10-07 用户定）。
   「其他 → xochitl」页不再有传书入口，只管字体。 */
function renderTransfer(sec){sec.innerHTML=`
  <div class="subnav"><button class="on">${T('transfer.subnav.intake')}</button><button>${T('transfer.subnav.library')}</button></div>
  <div class="subpanel on">
    <div class="card"><h2>${T('transfer.intake.title')}</h2>
      <p class="lead">${T('transfer.intake.lead')}</p>
      ${GUIDE()}
      ${onUsb()?'':`<p class="opt-note">${T('transfer.intake.usbHint')}</p>`}
    </div>
    <!-- 入库来源各自独立成卡（2026-09-10 用户要求：原来挤在同一张卡里用 h3 分隔，看着像
         "上传"下面附带子步骤，实际是几条互不依赖、各走各的入库路径，拆卡片才是"独立功能来源"
         该有的视觉分量，跟「系统增强」/「实验室」那种并排卡片同一个语言。2026-09-18：原第三张
         「电脑 shelf push」卡片随 host 整条线退役一并删除——用户明确表态以后不再使用 PC 端，
         入库只剩「上传」+「抓网文」两条路，都走网页本身，不用任何 host CLI。2026-10-07：「抓网文」随书架不再优化书删掉，
         网页入库只剩「上传」——网页链接请用 sheng-ren 收书、优化好再传）。 -->
    <div class="card"><h3 style="margin-top:0">${T('transfer.upload.title')}</h3>
      ${upHtml('⬆',T('transfer.upload.dropLabel',{native:up(EXT.book)}),BOOK_EXT,T('transfer.upload.btn'))}
      <p class="small">${T('transfer.upload.hint')}</p>
    </div>
  </div>
  <div class="subpanel" id="stgroot">
    <div class="card stg-head">
      <div class="stg-headrow"><h3 style="margin:0">${T('transfer.staging.title')}</h3><span class="small" id="stgcap"></span></div>
      <div class="stg-dest">
        <div class="stg-destcol"><label class="small" for="folder">${T('stg.dest.xochitl')}</label><select id="folder"></select></div>
        <span class="stg-newrow" id="xnew" hidden><input type="text" id="xnewname" placeholder="${T('stg.dest.newPlaceholder')}"><button class="btn pri" id="xnewgo">${T('stg.dest.create')}</button><button class="btn" id="xnewx">${T('stg.dest.cancel')}</button></span>
        <details class="cmp"><summary>${T('transfer.staging.optDetailsSummary')}</summary><p class="small">${T('transfer.staging.optNote')}</p></details>
      </div>
      <div class="small" id="stgfree"></div>
    </div>
    <div class="stg-tools"><input type="text" id="stgq" list="stgnames" autocomplete="off" placeholder="${T('transfer.staging.searchPlaceholder')}" aria-label="${T('transfer.staging.searchAria')}"><datalist id="stgnames"></datalist><select id="stgfmt" aria-label="${T('transfer.staging.fmtFilterAria')}"><option value="">${T('transfer.staging.fmtAll')}</option>${EXT.book.map(e=>`<option value="${e}">${e.toUpperCase()}</option>`).join('')}</select></div>
    <div class="stg-chips" id="stgchips"></div>
    <div class="stg-selrow"><label class="toggle"><input type="checkbox" id="stgall"> <span id="stgalltxt"></span></label></div>
    <ul class="stg-list" id="stglist"></ul>
    <div class="stg-pager" id="stgpager"></div>
    <div class="stgbar" id="stgbar" hidden></div>
  </div>`;
  let items=[];
  const picked=new Set();                                  // 勾选的书名（跨页保留）
  // 服务端批量队列状态（网关 /api/batch/status）：关掉页面重开、换设备都读得到，不再依赖本标签页提交过什么。
  let bs={running:false,waitingService:false,total:0,done:0,failed:[],queued:[],current:null,action:null};
  let batchQueued=new Set();
  const g=id=>$('#'+id,sec);
  // 加入位置：下拉（现有文件夹）+「＋新建文件夹」。选中值记在本机；"根目录"= 空串。新建走 book-serve 的 mkdir 队列，
  // 由 xochitl 里的 QML 代理（长轮询）真正建出来。**不在这里等它建好**：fillFolders 会把记住的名字补进下拉（哪怕 xochitl 那边
  // 还没出现），加入 xochitl 时 book-serve 自己会等这个文件夹建出来（deliver.rs::ensure_folder）；建好后 mkdir 事件
  // 触发的刷新把它换成真实列表里的那一项。此前这里每 2 秒查一次 /api/books/status、最多 12 次，按钮也跟着卡 24 秒。
  const NEW='__new__';
  const fsel=g('folder'),newBox=g('xnew'),newName=g('xnewname');
  const xFolder=()=>fsel.value===NEW?'':fsel.value;
  const fillFolders=names=>{const want=LS.get('folder','');const list=[...new Set(names.filter(Boolean))];if(want&&!list.includes(want))list.push(want);
    fsel.replaceChildren(el('option',{value:'',text:T('stg.dest.root')}),...list.map(n=>el('option',{value:n,text:n})),el('option',{value:NEW,text:T('stg.dest.new')}));
    fsel.value=newBox.hidden?want:NEW}; // 新建框还开着（正在填名字）时刷新别把下拉跳回旧值
  fsel.addEventListener('change',()=>{if(fsel.value===NEW){newBox.hidden=false;newName.focus()}else{newBox.hidden=true;LS.set('folder',fsel.value)}});
  g('xnewx').onclick=()=>{newBox.hidden=true;newName.value='';fsel.value=LS.get('folder','')};
  guardClick(g('xnewgo'),async()=>{const name=newName.value.trim();if(!name){toast(T('stg.dest.needName'),'warn');return}
    const r=await postJ('/api/books/mkdir/add',{name});if(r.ok===false)return;
    toast(T('stg.dest.created',{name}),'info',6000);
    LS.set('folder',name);newBox.hidden=true;newName.value='';await refresh()});
  // 筛选/分页状态。默认「未加入」（顶替原来"全部 + 隐藏已完成"的默认视图）；旧版存的「已优化」筛选（done）也回落到它。
  let st=['all','todo','finished'].includes(LS.get('stgSt','todo'))?LS.get('stgSt','todo'):'todo',page=1,pageSize=+LS.get('stgPageSize','25')||25;
  const filtered=()=>{const q=g('stgq').value.toLowerCase(),f=g('stgfmt').value;
    return items.filter(it=>(!q||it.name.toLowerCase().includes(q))&&(!f||it.format===f)&&(st==='todo'?stgIsTodo(it):st==='finished'?isBookDone(it):true))};
  const batchTitle=a=>T('stg.batch.'+a);
  const enqueue=async(action,body)=>{if(fsel.value===NEW){toast(T('stg.dest.createFirst'),'warn');return}
    const r=await postJ('/api/batch',{action,folder:xFolder(),...body});
    if(r.ok===false)return;
    toast(r.queued?T('stg.batch.queuedToast',{queued:r.queued,skip:r.skipped?T('stg.batch.skipped',{n:r.skipped}):''}):T('stg.batch.none'),r.queued?'ok':'warn');
    if(r.queued)picked.clear();await refresh()};
  const ctx=()=>({picked,batchQueued,bs,syncSel:()=>{syncSelUi();renderBar()}});
  const syncSelUi=()=>{const list=filtered();const n=list.length,all=n>0&&list.every(it=>picked.has(it.name));
    g('stgall').checked=all;g('stgall').indeterminate=!all&&list.some(it=>picked.has(it.name));g('stgalltxt').textContent=T('stg.selectAll',{n})};
  g('stgall').onchange=()=>{const list=filtered();if(g('stgall').checked)list.forEach(it=>picked.add(it.name));else list.forEach(it=>picked.delete(it.name));render()};
  const renderChips=()=>{const chips=g('stgchips');chips.innerHTML='';
    // 三个筛选统一都带数量（数量 = 该筛选下的书本数）。
    const cnt={all:items.length,todo:items.filter(stgIsTodo).length,finished:items.filter(isBookDone).length};
    [['all','stg.chip.all'],['todo','stg.chip.todo'],['finished','stg.chip.finished']].forEach(([k,key])=>{
      chips.appendChild(btn(T(key,{n:cnt[k]}),()=>{st=k;LS.set('stgSt',k);page=1;render()},'chip'+(st===k?' on':'')))})};
  const renderPager=(total)=>{const box=g('stgpager');box.innerHTML='';if(total<=0)return;
    const pages=Math.max(1,Math.ceil(total/pageSize));const from=(page-1)*pageSize+1,to=Math.min(total,page*pageSize);
    const go=p=>{page=Math.min(pages,Math.max(1,p));render();g('stglist').scrollIntoView({block:'start'})};
    const prev=btn('‹ '+T('stg.pager.prev'),()=>go(page-1));prev.disabled=page<=1;
    const next=btn(T('stg.pager.next')+' ›',()=>go(page+1));next.disabled=page>=pages;
    box.appendChild(el('div',{class:'small stg-range',text:T('stg.pager.range',{from,to,total})}));
    const nav=el('div',{class:'stg-pgnav'},[prev]);
    // 页码：首页、当前页±1、末页，中间省略——宽屏显示数字，窄屏只留"第 x/y 页"
    const nums=[...new Set([1,page-1,page,page+1,pages].filter(p=>p>=1&&p<=pages))].sort((a,b)=>a-b);
    let last=0;const numbox=el('span',{class:'stg-pgnums'});
    nums.forEach(p=>{if(last&&p-last>1)numbox.appendChild(el('span',{class:'small',text:'…'}));numbox.appendChild(btn(String(p),()=>go(p),'btn'+(p===page?' pri':'')));last=p});
    nav.appendChild(numbox);nav.appendChild(el('span',{class:'small stg-pgtxt',text:T('stg.pager.page',{cur:page,total:pages})}));nav.appendChild(next);box.appendChild(nav);
    const sz=el('select',{'aria-label':T('stg.pager.size',{n:pageSize})});[25,50,100].forEach(n=>{const o=el('option',{value:String(n),text:T('stg.pager.size',{n})});if(n===pageSize)o.selected=true;sz.appendChild(o)});
    sz.onchange=()=>{pageSize=+sz.value;LS.set('stgPageSize',String(pageSize));page=1;render()};box.appendChild(sz)};
  /* 底部操作栏，最多三块，从上到下：① 批量队列还有活（在跑，或网关重启后等 book-serve 就绪）＝进度 + 全部中止；
     ② 有勾选＝选择操作（批量运行期间照样能删、改名、下载、往队列里追加——此前跑批量时整栏只剩进度，几十本大书一跑
     就是很久，母版库没法操作）；③ 都没有、上一轮已跑完＝结果小结（可收起，失败原因可展开）。收起记在本机：服务端会一直
     保存上一轮结果（网关重启也读回），只记在内存里的话每次打开页面都会重新弹出来。 */
  const renderBar=()=>{const bar=g('stgbar');bar.innerHTML='';
    const sig=`${bs.action}|${bs.total}|${bs.done}|${bs.failed.length}`;
    const live=bs.running||bs.queued.length>0;
    if(live){
      const t=batchTitle(bs.action||'deliver');
      // 在跑但还没取到第一本（`waitingService`：网关重启后等 book-serve 起来），或有排队却没有 worker（等了 30 分钟
      // 仍没等到 book-serve、队列留着待下次入队带起）——都如实说"在等"，不显示成"正在处理"。
      const waiting=!!bs.waitingService||(!bs.running&&bs.queued.length>0);
      const head=waiting?T('stg.batch.waiting',{n:bs.queued.length}):T('stg.batch.progress',{title:t,done:bs.done,total:bs.total});
      const sub=(bs.current?' · '+T('stg.batch.current',{name:stgClean(bs.current)}):'')+(bs.failed.length?' · '+T('stg.batch.failedN',{n:bs.failed.length}):'');
      const p=el('progress');p.max=Math.max(1,bs.total);p.value=bs.done;
      const stop=btn(T('stg.batch.stopAll'),async()=>{const r=await postJ('/api/batch/stop',{});if(r.ok!==false)toast(T('stg.batch.stopped',{n:r.cleared||0}),'info');await refresh()},'btn btn-bad');
      bar.appendChild(el('div',{class:'stgbar-run'},[el('div',{class:'stgbar-main'},[el('b',{text:head}),el('span',{class:'small',text:sub})]),p,stop]));
    }
    if(picked.size){
      // 第一行：已选数 + 清除；第二行：加入 xochitl（主操作，按钮上标可处理数）；第三行：单本操作 + 删除。
      const chosen=items.filter(it=>picked.has(it.name));
      bar.appendChild(el('div',{class:'stgbar-top'},[el('b',{text:T('stg.selected',{n:picked.size})}),btn(T('stg.batch.clear'),()=>{picked.clear();render()})]));
      // 按钮文案 = 标签 + 数量角标；窄屏去掉"加入"前缀（.lbl-long），一行三个也放得下（2026-09-24 用户要求手机上不折行）。
      const lbl=(key,n)=>{const f=document.createDocumentFragment();const t=T(key);const m=t.match(/^(加入 |Add to )(.*)$/);
        if(m){f.appendChild(el('span',{class:'lbl-long',text:m[1]}));f.appendChild(document.createTextNode(m[2]))}else f.appendChild(document.createTextNode(t));
        if(n!=null)f.appendChild(el('span',{class:'cnt',text:String(n)}));return f};
      const add=btn([lbl('stg.bar.deliver',chosen.length)],()=>enqueue('deliver',{names:[...picked]}),'btn pri',{title:T('common.paren',{text:T('stg.bar.deliver'),inner:chosen.length})});
      const btns=el('div',{class:'stgbar-btns'},[add]);btns.style.setProperty('--cols','1');
      const free=chosen.filter(it=>!it.busy).map(it=>it.name),busyN=chosen.length-free.length;
      const del=btn([lbl('action.delete',free.length)],free.length?async()=>{
        if(!await confirmDialog(T('stg.batch.deleteConfirm',{n:free.length})))return;
        let ok=0;for(const n of free){const r=await jsend('/api/books/staging/delete','POST',{name:n});if(r.ok!==false)ok++}
        toast(T('stg.batch.deleted',{n:ok})+(busyN?T('stg.batch.deleteSkipped',{n:busyN}):''),ok?'ok':'warn');picked.clear();refresh()}:null,'btn btn-bad',{title:T('common.paren',{text:T('action.delete'),inner:free.length})});
      if(!free.length){del.disabled=true;del.title=T('stg.bar.allBusy')}
      bar.appendChild(btns);
      const row3=el('div',{class:'stgbar-btns stgbar-sub'});
      // 只选了一本：再给「下载原件」「改名」（都是针对单本的操作，多选时不出现），删除排在同一行最右。
      if(chosen.length===1){const one=chosen[0];
        const dl=el('a',{class:'btn',href:'/api/books/staging/file?name='+encodeURIComponent(one.name),download:one.name,text:T('stg.bar.download')});
        const rn=btn(T('stg.bar.rename'),one.busy?null:async()=>{const ext='.'+one.name.split('.').pop();
          const v=await promptDialog(T('stg.rename.prompt',{ext}),one.name.slice(0,-ext.length));
          if(v==null||!v.trim())return;
          const r=await postJ('/api/books/staging/rename',{name:one.name,newName:v.trim()});
          if(r.ok!==false){picked.clear();picked.add(r.name);toast(T('stg.rename.done',{name:r.name}),'ok')}refresh()});
        if(one.busy){rn.disabled=true;rn.title=T('stg.bar.busy')}
        row3.append(dl,rn)}
      row3.append(del);row3.style.setProperty('--cols','3');bar.appendChild(row3);
    }else if(!live&&bs.total&&sig!==LS.get('stgDismissed','')){
      const fail=bs.failed.length;
      bar.appendChild(el('div',{class:'stgbar-main'},[el('span',{text:T('stg.batch.finished',{title:batchTitle(bs.action||'deliver'),ok:bs.done-fail,fail})})]));
      if(fail)bar.appendChild(el('details',{class:'small stgbar-fails'},[el('summary',{text:T('stg.batch.failedN',{n:fail})}),el('div',{html:bs.failed.map(f=>`<div>${esc(T('common.labelValue',{label:stgClean(f.name),value:f.message}))}</div>`).join('')})]));
      bar.appendChild(btn(T('stg.batch.dismiss'),()=>{LS.set('stgDismissed',sig);renderBar()}));
    }
    bar.hidden=!bar.children.length;
    bar.className='stgbar'+(picked.size?' sel':live?' run':' done')};
  const render=()=>{
    const list=filtered();const pages=Math.max(1,Math.ceil(list.length/pageSize));if(page>pages)page=pages;
    const ul=g('stglist');ul.innerHTML='';
    if(!list.length)ul.appendChild(el('li',{class:'small stg-empty',text:items.length?T('transfer.staging.emptyFiltered'):T('transfer.staging.emptyAll')}));
    else{const c=ctx();list.slice((page-1)*pageSize,page*pageSize).forEach(it=>ul.appendChild(stgRow(it,c)))}
    renderChips();renderPager(list.length);syncSelUi();renderBar()};
  // 搜索框每敲一个字符都整表重画（最多 100 行）太浪费：input 防抖 150ms，change（失焦/回车/选中建议）与格式下拉立即生效。
  let qTimer=0;const rerender=()=>{clearTimeout(qTimer);page=1;render()};
  g('stgq').addEventListener('input',()=>{clearTimeout(qTimer);qTimer=setTimeout(rerender,150)});
  g('stgq').addEventListener('change',rerender);
  g('stgfmt').addEventListener('change',rerender);
  const refresh=()=>refreshAt(3);
  // 网关自身的批量队列状态（见 batch.rs）。
  const applyQueue=bt=>{
    if(bt.ok!==false){bs={running:!!bt.running,waitingService:!!bt.waitingService,action:bt.action,total:bt.total||0,done:bt.done||0,current:bt.current,queued:bt.queued||[],failed:bt.failed||[]};batchQueued=new Set(bs.queued)}};
  /* 按事件决定取多少（不轮询）。三档，数字越大取得越全：
     1 = 网关自己的批量队列事件（area=books、不带 svc）：只重取队列状态（1 个请求）。
     2 = book-serve 的 `staging` 事件（入库、忙态开始/结束、落库结果）与 `render` 事件（渲染自检结果）：只有母版库列表会变，
         再加队列状态（2 个请求）；xochitl 文件夹列表不会因此变化，不重取。
     3 = 其余（book-serve 的 mkdir/trash/inbox 事件、切 tab、重连、操作后主动刷新）：全量 3 个请求。
     所有刷新走同一个 coalesce 串行执行（need 记"下一轮至少要取到哪一档"，取最大），不会出现旧的全量结果盖掉新的排队状态。 */
  let need=0;
  const run=coalesce(async()=>{const lvl=need;need=0;if(!lvl)return;
    if(lvl===1){applyQueue(await j('/api/batch/status'));render();return}
    const full=lvl>=3;
    const [d,bt,s]=await Promise.all([j('/api/books/staging'),j('/api/batch/status')].concat(full?[j('/api/books/status')]:[]));
    if(full)fillFolders(s.ok!==false?s.xochitlFolders||[]:[]);
    applyQueue(bt);
    if(d.ok===false){items=[];render();g('stgcap').textContent='';g('stgfree').textContent='';g('stglist').innerHTML=`<li class="small stg-empty" style="color:var(--bad)">${esc(T('transfer.staging.unavailable',{msg:d.message||T('transfer.staging.notOpen')}))}</li>`;return}
    items=d.items||[];const tot=items.reduce((a,b)=>a+b.bytes,0);g('stgcap').textContent=items.length?T('transfer.staging.capSummary',{count:items.length,size:fmtB(tot)}):'';
    // 清掉选中集合里的幽灵条目（书被改名/删除后旧名字再也选不中也取消不掉）
    const names=new Set(items.map(it=>it.name));for(const n of [...picked])if(!names.has(n))picked.delete(n);
    const fr=d.freeBytes;const low=stagingLowSpace(d);g('stgfree').style.color=low?'var(--bad)':'';g('stgfree').textContent=fr!=null?T('transfer.staging.freeSpace',{free:fmtB(fr),lowWarn:low?T('transfer.staging.lowWarn'):''}):'';
    g('stgnames').innerHTML=stgNameOptions(items);render()});
  const refreshAt=lvl=>{need=Math.max(need,lvl);return run()};
  uploader($('.up',sec),'/api/books/staging',BOOK_EXT,()=>refresh(),'/api/books/staging');   // 书籍格式原样入库；选中即按 BOOK_EXT 拦；传 dedupeApi 防重传出重复
  refresh();sec.refresh=refresh;sec.onEvent=ev=>refreshAt(ev.area!=='books'?3:!ev.svc&&ev.kind==='batch'?1:ev.kind==='staging'||ev.kind==='render'?2:3);subtabs(sec);}
