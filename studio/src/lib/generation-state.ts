export function phaseRows(run: any) {
  return Array.from({length:run?.phaseCount??6}, (_,i)=>({n:i+1,...run?.phases?.[String(i+1)],state:run?.phases?.[String(i+1)]?.state??'waiting'}));
}
// A phase in one of these states can be submitted again with the same seed (core `saya_resume`).
// `submission_unknown` is what a ComfyUI crash or restart leaves: the core confirms the prompt is
// gone before replacing it.
const RESUMABLE=['failed','interrupted','cancelled','submission_unknown'];
const ENGINE_BUSY=['submitted','submitting','queued','running'];
export function runControls(run:any) {
  const phases=phaseRows(run);
  const active=phases.some(p=>[...ENGINE_BUSY,'submission_unknown'].includes(p.state));
  const next=phases.find(p=>p.state!=='succeeded')?.n??7;
  const saya=!run?.singleJob&&!!run;
  // Resume: nothing ComfyUI may still execute, and either a pass that failed, was interrupted or
  // was lost, or a stopped run between two passes.
  const resume=saya&&next<=6&&!phases.some(p=>ENGINE_BUSY.includes(p.state))
    &&(RESUMABLE.includes(phases[next-1].state)||(run.state==='stopped'&&next>1));
  // Stop stays available until the run is finished: phase in progress (interrupted) or between two phases.
  return {stop:active||(!!run&&run.state==='running'),continue:saya&&run.state==='running'&&!active&&next>1&&next<=6&&phases[next-1].state==='waiting',restart:saya&&!active,resume,next};
}
export function randomSeed() {
  const bytes=new Uint32Array(2);crypto.getRandomValues(bytes);
  return ((BigInt(bytes[0])<<32n|BigInt(bytes[1]))%1125899906842624n).toString();
}
export function resolveSeed(mode:string,value:string,last:string) {
  let seed=value.trim();
  if(mode==='randomize'||seed==='-1')return randomSeed();
  if(mode==='last'){if(!last)throw Error('Aucune seed réellement soumise pour ce workflow.');seed=last;}
  if(mode==='increment'||mode==='decrement'||seed==='-2'||seed==='-3'){
    const increment=mode==='increment'||seed==='-2';
    if(!last)return randomSeed();
    const n=BigInt(last);seed=(increment?n+1n:n>0n?n-1n:0n).toString();
  }
  if(!seed)return '';
  if(!/^\d+$/.test(seed)||BigInt(seed)>18446744073709551615n)throw Error('Seed entière u64 requise (0 à 18446744073709551615).');
  return BigInt(seed).toString();
}
