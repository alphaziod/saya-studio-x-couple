(async()=>{
 // Casting names, duo and delivered files (see casting-naming-native-smoke.sh). Checked through the
 // DOM and the app's own IPC; no pointer is used and no image is read.
 const report={ok:false,checks:[]};
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const pause=ms=>new Promise(r=>setTimeout(r,ms));
 const wait=async(fn,m,ms=240000)=>{window.__SMOKE_STEP=m;const end=Date.now()+ms;while(!await fn()){if(Date.now()>end)throw Error(m);await pause(250)}};
 const press=async s=>{await wait(()=>document.querySelector(s)&&!document.querySelector(s).disabled,s);document.querySelector(s).click()};
 const go=async id=>{await wait(()=>document.querySelector(`[data-nav="${id}"]`),'nav '+id);document.querySelector(`[data-nav="${id}"]`).click()};
 const set=(s,v)=>{const n=document.querySelector(s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}))};
 const check=(name,ok,detail='')=>{report.checks.push({name,ok:!!ok,detail:String(detail)});if(!ok)throw Error(`${name}: ${detail}`)};
 const history=()=>api('casting_history',{sync:true});
 const input=()=>document.querySelector('[data-casting-panel] .wb-panel.right [data-casting-save-nom]');
 try{
  await wait(async()=>!!(await api('preferences')).language||!!document.querySelector('[data-langue-setup]'),'language ready');
  if(document.querySelector('[data-langue-setup]')){document.querySelector('[data-langue="fr"]').click();document.querySelector('[data-langue-confirmer]').click();await wait(()=>!document.querySelector('[data-langue-setup]'),'language chosen')}
  await go('generation');await wait(()=>document.querySelector('[data-saya-source]'),'source');
  set('[data-saya-source]',window.__V15_UI_WORKFLOW__);await press('[data-workflow-open]');
  await wait(async()=>!!(await api('preferences')).generationWorkspace?.session,'opened session');
  await go('casting');
  await wait(()=>document.querySelector('[data-casting-format]'),'duo offered by the engine');
  check('solo by default',document.querySelector('[data-casting-format]').value==='solo');
  check('no pair choice in solo',!document.querySelector('[data-casting-duo]'));
  await wait(()=>!document.querySelector('[data-casting-lancer]').disabled,'launch ready');
  set('[data-casting-n]','2');await pause(100);
  await press('[data-casting-lancer]');
  await wait(async()=>{const r=await history();return r.length===2&&r.every(x=>x.asset)},'two trials rendered');
  const rows=await history();
  for(const r of rows){
   check('engine name replaced',r.shot.engine_name&&r.name!==r.shot.engine_name&&!/Stub/.test(r.name),`${r.shot.engine_name} -> ${r.name}`);
   check('style from the family',r.shot.name_style==='bny',r.shot.name_style);
  }
  check('names differ',rows[0].name!==rows[1].name,rows.map(r=>r.name));
  report.names=rows.map(r=>r.name);
  await wait(()=>input()&&input().value,'name prefilled');
  const shown=document.querySelector('[data-casting-panel] .wb-panel.right .wb-head h2').textContent.trim();
  check('prefilled with the shown trial',input().value===shown,`${input().value} / ${shown}`);
  const other=rows.find(r=>r.name!==shown);
  // The grid refreshes on its own timer: this IPC history can be ahead of it.
  await wait(()=>document.querySelector(`[data-casting-shot="${CSS.escape(other.stem)}"] .tile-img`),'other trial tile',15000);
  document.querySelector(`[data-casting-shot="${CSS.escape(other.stem)}"] .tile-img`).click();
  await wait(()=>input().value===other.name,'another trial prefills its name',5000);
  set('[data-casting-panel] .wb-panel.right [data-casting-save-nom]','Essai Perso');
  document.querySelector('[data-casting-panel] .lots-head .icon-btn').click();
  await pause(4000);
  check('a refresh keeps what was typed',input().value==='Essai Perso',input().value);
  await press('[data-casting-panel] .wb-panel.right [data-casting-enregistrer]');
  await wait(()=>/Essai Perso/.test(document.querySelector('[data-casting-enregistre-note]')?.textContent??''),'sheet saved');
  await wait(async()=>(await history()).some(r=>r.run===other.run&&r.name==='Essai Perso'),'history carries the sheet name');
  check('tile shows the sheet name',[...document.querySelectorAll('[data-casting-panel] .tile .meta h2')].some(n=>n.textContent==='Essai Perso'));
  await wait(async()=>(await history()).find(r=>r.run===other.run)?.phases?.['6']?.state==='succeeded','six passes finished');
  const finished=(await history()).find(r=>r.run===other.run);
  const decision=await api('saya_review_decision',{job:finished.phases['1'].job,decision:'validate',confirmed:true});
  check('review decision kept',decision.action==='validate'&&decision.phase===1,JSON.stringify(decision));
  check('final image known',!!finished.finalAsset);
  set('[data-casting-format]','duo');await pause(200);
  check('three pair types',document.querySelectorAll('[data-casting-duo] option').length===3);
  check('solo options locked in duo',document.querySelector('[data-casting-famille]').disabled&&document.querySelector('[data-casting-cibler]').disabled&&document.querySelector('[data-casting-step-start]').disabled);
  set('[data-casting-duo]','girl_x_girl');set('[data-casting-n]','2');await pause(100);
  const before=(await history()).length;
  await press('[data-casting-lancer]');
  await wait(async()=>{const r=await history();return r.length===before+2&&r.filter(x=>x.shot?.duo).every(x=>x.asset)},'two duos rendered');
  const duos=(await history()).filter(r=>r.shot?.duo);
  check('two duo trials',duos.length===2,duos.length);
  check('duo keeps both names',duos.every(r=>r.name==='Alba Test x Rhea Test'&&!r.shot.engine_name),duos.map(r=>r.name));
  check('duo fills P2',duos.every(r=>/duo-fixture-p2/.test(r.shot.casting.p2)&&r.shot.casting.duo===true));
  await wait(()=>document.querySelector(`[data-casting-shot="${CSS.escape(duos[0].stem)}"] .tile-img`),'duo tile');
  document.querySelector(`[data-casting-shot="${CSS.escape(duos[0].stem)}"] .tile-img`).click();
  await wait(()=>document.querySelector('[data-casting-panel] .wb-panel.right [data-casting-duo-note]'),'duo note',5000);
  check('no sheet input on a duo',!input());
  const refused=await api('casting_enregistrer_perso',{run:duos[0].run,stem:duos[0].stem,nom:'Refus'}).then(()=>false,()=>true);
  check('saving a duo as one sheet is refused',refused);
  check('no error shown',!document.querySelector('[data-casting-panel] .strip.error'),document.querySelector('[data-casting-panel] .strip.error')?.textContent);
  report.ok=true;
 }catch(e){report.failure=String(e?.message||e);report.step=window.__SMOKE_STEP;report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}
 await invoke('finish_smoke',{report});
})();
