import {test} from 'node:test';
import assert from 'node:assert/strict';
import {phaseRows, runControls, resolveSeed} from '../src/lib/generation-state.ts';
test('six phases use backend states, one next action, no continue while queued',()=>{
 const run={state:'running',phases:{'1':{state:'succeeded'},'2':{state:'queued'}}};
 assert.equal(phaseRows(run).length,6);
 assert.equal(phaseRows(run)[1].state,'queued');
 assert.equal(phaseRows(run)[5].state,'waiting');
 assert.deepEqual(runControls(run),{stop:true,continue:false,restart:false,resume:false,next:2});
 run.phases['2'].state='succeeded';
 assert.equal(runControls(run).next,3);assert.equal(runControls(run).continue,true);
 assert.equal(runControls({...run,state:'stopped'}).continue,false);
 assert.equal(runControls({...run,state:'completed'}).continue,false);
 assert.equal(runControls({state:'completed',phases:Object.fromEntries(Array.from({length:6},(_,i)=>[i+1,{state:'succeeded'}]))}).restart,true);
});
test('resume is offered after a crash, a failure or a stop, never while ComfyUI may still run',()=>{
 const done={state:'succeeded'};
 const run=(state,third)=>({state,phases:{'1':done,'2':done,'3':{state:third}}});
 assert.equal(runControls(run('running','submission_unknown')).resume,true,'prompt lost by a ComfyUI restart');
 assert.equal(runControls(run('running','failed')).resume,true);
 assert.equal(runControls(run('stopped','interrupted')).resume,true);
 assert.equal(runControls(run('stopped','waiting')).resume,true,'stopped between two passes');
 assert.equal(runControls(run('running','waiting')).resume,false,'Continue covers a running run');
 for(const busy of ['submitted','submitting','queued','running'])assert.equal(runControls(run('running',busy)).resume,false,busy);
 assert.equal(runControls({...run('running','failed'),singleJob:true}).resume,false);
 assert.equal(runControls(run('stopped','interrupted')).next,3);
});
test('seed resolution is exact and never changes last queued at preview',()=>{
 assert.equal(resolveSeed('fixed','9007199254740993','10'),'9007199254740993');
 assert.equal(resolveSeed('increment','10','9007199254740993'),'9007199254740994');
 assert.equal(resolveSeed('decrement','10','0'),'0');
 assert.equal(resolveSeed('last','10','9007199254740993'),'9007199254740993');
 assert.throws(()=>resolveSeed('last','10',''));
 assert.throws(()=>resolveSeed('fixed','18446744073709551616',''));
 assert.throws(()=>resolveSeed('fixed','bad',''));
});
