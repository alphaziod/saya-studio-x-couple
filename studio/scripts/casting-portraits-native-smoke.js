(async()=>{
 // « Images manquantes » (see casting-portraits-native-smoke.sh). Every step is checked through the
 // DOM and the app's own IPC; no pointer is used.
 const report={ok:false,checks:[]};
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const wait=async(fn,m,ms=120000)=>{window.__SMOKE_STEP=m;const end=Date.now()+ms;while(!await fn()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,200))}};
 const press=async s=>{await wait(()=>document.querySelector(s)&&!document.querySelector(s).disabled,s);document.querySelector(s).click()};
 const go=async id=>{await wait(()=>document.querySelector(`[data-nav="${id}"]`),'nav '+id);document.querySelector(`[data-nav="${id}"]`).click()};
 const set=(s,v)=>{const n=document.querySelector(s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}))};
 const check=(name,ok,detail='')=>{report.checks.push({name,ok:!!ok,detail});if(!ok)throw Error(`${name}: ${detail}`)};
 const left=()=>Number(document.querySelector('[data-casting-portraits-state]')?.textContent.match(/\d+/)?.[0]??-1);
 const history=()=>api('casting_history',{sync:true});
 try{
  await wait(async()=>!!(await api('preferences')).language||!!document.querySelector('[data-langue-setup]'),'language ready');
  if(document.querySelector('[data-langue-setup]')){document.querySelector('[data-langue="fr"]').click();document.querySelector('[data-langue-confirmer]').click();await wait(()=>!document.querySelector('[data-langue-setup]'),'language set')}
  await go('generation');await wait(()=>document.querySelector('[data-saya-source]'),'source');
  set('[data-saya-source]',window.__V15_UI_WORKFLOW__);await press('[data-workflow-open]');
  await wait(async()=>!!(await api('preferences')).generationWorkspace?.session,'opened session');
  await go('casting');await press('[data-casting-cibler]');
  await wait(()=>document.querySelectorAll('[data-cibler-perso]').length===3,'three characters listed');
  const names=[...document.querySelectorAll('[data-cibler-perso] strong')];
  check('names visible',names.every(n=>n.getBoundingClientRect().height>=10&&n.textContent.trim()),names.map(n=>Math.round(n.getBoundingClientRect().height)).join(','));
  check('missing button counts three',/\(3\)/.test(document.querySelector('[data-cibler-portraits]')?.textContent??''),document.querySelector('[data-cibler-portraits]')?.textContent);
  await press('[data-cibler-portraits]');
  await wait(()=>document.querySelector('[data-casting-portraits]')&&!document.querySelector('[data-cibler-fenetre]'),'portrait panel open');
  await wait(()=>!document.querySelector('[data-portrait-yes]').disabled,'first attempt image');
  check('three left at start',left()===3,left());
  const before=(await history()).length;
  await press('[data-portrait-no]');
  await wait(async()=>(await history()).length===before+1,'No drew another attempt');
  await wait(()=>!document.querySelector('[data-portrait-yes]').disabled,'second attempt image');
  check('No keeps the same character',left()===3,left());
  await press('[data-portrait-yes]');
  await wait(()=>left()===2,'Yes moves to the next character');
  const kept=(await history()).filter(r=>r.decision==='oui'&&r.published);
  check('Yes finished and kept one trial',kept.length===1,JSON.stringify(kept.map(r=>r.run)));
  const previews=await api('casting_previews');
  check('kept image pictures the character',!!previews.persos['CH--9201'],JSON.stringify(previews.persos));
  await wait(()=>!document.querySelector('[data-portrait-skip]').disabled,'skip ready');
  await press('[data-portrait-skip]');
  await wait(()=>left()===1,'Skip moves on');
  await press('[data-portrait-quit]');
  await wait(()=>!document.querySelector('[data-casting-portraits]'),'panel closed');
  check('no error shown',!document.querySelector('.error'),document.querySelector('.error')?.textContent);
  report.ok=true;
 }catch(e){report.failure=String(e?.message||e);report.step=window.__SMOKE_STEP;report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}
 await invoke('finish_smoke',{report});
})();
