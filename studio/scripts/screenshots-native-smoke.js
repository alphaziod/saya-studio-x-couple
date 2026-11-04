(async()=>{
 // README screenshots in an isolated profile (stub engine, mock ComfyUI serving the real PNGs of
 // MOCK_COMFY_IMAGES). First launch prepares everything (a full six-pass run, a casting batch, kept
 // images); the restart opens the app on that data and only takes the captures: Generation, Casting,
 // Gallery. Captures land in the isolated data dir.
 const report={ok:false,checks:[],captures:{}};
 const invoke=window.__TAURI_INTERNALS__.invoke,api=(action,args={})=>invoke('studio',{request:{action,...args}});
 const wait=async(fn,m,ms=90000)=>{window.__SMOKE_STEP=m;const end=Date.now()+ms;while(!await fn()){if(Date.now()>end)throw Error(m);await new Promise(r=>setTimeout(r,150))}};
 const press=async s=>{await wait(()=>document.querySelector(s)&&!document.querySelector(s).disabled,s);document.querySelector(s).click()};
 const go=async id=>{await wait(()=>document.querySelector(`[data-nav="${id}"]`),'nav '+id);document.querySelector(`[data-nav="${id}"]`).click()};
 const set=(s,v)=>{const n=document.querySelector(s);n.value=v;n.dispatchEvent(new Event('input',{bubbles:true}));n.dispatchEvent(new Event('change',{bubbles:true}))};
 const loaded=sel=>[...document.querySelectorAll(sel)].filter(i=>i.complete&&i.naturalWidth>64).length;
 // The real pointer may rest over the window: disable pointer events so no :hover state or
 // tooltip lands in a capture. Programmatic clicks of this script are unaffected.
 const ignorePointer=()=>{if(document.getElementById('smoke-no-pointer'))return;const style=document.createElement('style');style.id='smoke-no-pointer';style.textContent='*{pointer-events:none!important}';document.head.append(style)};
 const shot=async name=>{ignorePointer();await new Promise(r=>setTimeout(r,2000));report.captures[name]=await invoke('smoke_capture',{name})};
 try{
  await wait(async()=>!!(await api('preferences')).language||!!document.querySelector('[data-langue-setup]'),'language ready');
  if(document.querySelector('[data-langue-setup]')){document.querySelector('[data-langue="fr"]').click();document.querySelector('[data-langue-confirmer]').click();await wait(()=>!document.querySelector('[data-langue-setup]'),'language set')}
  if(window.__V12_SMOKE_PHASE__!=='restart'){
   // Preparation, no capture.
   await go('generation');await wait(()=>document.querySelector('[data-saya-source]'),'source');
   set('[data-saya-source]',window.__V15_UI_WORKFLOW__);await press('[data-workflow-open]');
   await wait(async()=>!!(await api('preferences')).generationWorkspace?.session,'opened session');
   await press('[data-saya-start]');
   await wait(()=>document.querySelector('[data-saya-review-popup]'),'phase 1 review',180000);
   await press('[data-review-continue]');
   await wait(()=>document.querySelector('[data-saya-phase="6"]')?.dataset.state==='succeeded','six passes',300000);
   await go('casting');await wait(()=>document.querySelector('[data-casting-lancer]'),'casting ready');
   set('[data-casting-n]','8');await press('[data-casting-lancer]');
   await wait(async()=>(await api('casting_history',{sync:true})).filter(r=>r.asset).length>=8,'eight casting images',420000);
   // Eight trials, as in the README captures. The API can report the images before the grid
   // renders their tiles: select from the DOM.
   await wait(()=>document.querySelectorAll('[data-casting-select]').length>=8,'eight casting tiles',60000);
   for(const b of [...document.querySelectorAll('[data-casting-select]')].slice(0,8))b.click();
   await press('[data-casting-publish-selection]');
   await wait(async()=>(await api('casting_history',{sync:false})).filter(r=>r.published).length>=8,'kept in gallery');
   // Second Generation run (README: two runs in the session, the latest one shown). The README
   // image set repeats one casting render at the next cycle positions for it.
   await go('generation');await wait(()=>document.querySelector('[data-saya-start]')&&!document.querySelector('[data-saya-start]').disabled,'generation ready');
   await press('[data-saya-start]');
   await wait(()=>document.querySelector('[data-saya-review-popup]'),'second run review',180000);
   await press('[data-review-continue]');
   await wait(async()=>(await api('saya_runs')).filter(r=>!r.casting&&r.state==='completed').length>=2,'second run done',300000);
  }else{
   // Restart: everything is already there; only the captures.
   await go('generation');await wait(()=>document.querySelector('[data-saya-state]'),'run restored');
   await wait(()=>loaded('main img')>0,'generation preview',60000);
   // Capture after the status poll settles (controls and session thumbnails are dimmed while syncing).
   await wait(()=>document.querySelector('[data-saya-restart]')&&!document.querySelector('[data-saya-restart]').disabled,'run synced',30000);
   // The workflow field shows the machine's folders: hide its text in the capture only (the UI
   // re-renders a cleared value, a style survives).
   const mask=document.createElement('style');mask.textContent='[data-saya-source]{color:transparent!important}';document.head.append(mask);
   await shot('readme-1-generation');
   await go('casting');await wait(()=>loaded('[data-casting-shot] img')>=8,'casting images',60000);
   set('[data-casting-n]','8');  // the trial count shown matches the batch on screen
   await shot('readme-2-casting');
   await go('gallery');await wait(()=>loaded('img')>=8,'gallery images',60000);
   await shot('readme-3-gallery');
  }
  report.ok=true;
 }catch(e){report.failure=String(e?.message||e);report.step=window.__SMOKE_STEP;report.uiErrors=[...document.querySelectorAll('.error')].map(n=>n.textContent)}
 await invoke('finish_smoke',{report});
})();
