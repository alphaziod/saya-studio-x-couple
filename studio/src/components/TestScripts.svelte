<script lang="ts">
  import { onMount } from 'svelte';
  import { call, message } from '../lib/api';
  import { t } from '../lib/i18n';
  import Icon from './Icon.svelte';

  let { busy = $bindable(false), disabled = false } = $props<{busy?:boolean;disabled?:boolean}>();
  type TestScript = {id:string;label:string;path:string};
  let scripts = $state<TestScript[]>([]), selected = $state('');
  let loading = $state(false), running = $state(false), error = $state(''), output = $state(''), status = $state('');
  const selectedScript = $derived(scripts.find(script=>script.id===selected));

  async function refresh() {
    if(loading||running)return;
    loading=true;error='';
    try {
      const result=await call<{scripts:TestScript[]}>('engine_test_scripts');
      scripts=result.scripts;
      if(!scripts.some(script=>script.id===selected))selected='';
    }catch(e){error=message(e)}finally{loading=false}
  }
  async function generate() {
    if(busy||disabled||running||loading||!selectedScript)return;
    busy=true;running=true;error='';output='';status='';
    try {
      const result=await call<{success:boolean;timeout:boolean;code:number|null;stdout:string;stderr:string}>('engine_test_script',{script:selected});
      output=[result.stdout,result.stderr].filter(Boolean).join('\n');
      if(result.timeout)error=$t("Test interrompu : délai dépassé.");
      else if(!result.success)error=$t("Échec du test (code {code}).",{code:result.code??'?'});
      else status=$t("Terminé");
    }catch(e){error=message(e)}finally{busy=false;running=false}
  }
  onMount(()=>{void refresh()});
</script>

<details class="sec" data-test-scripts ontoggle={e=>{if(e.currentTarget.open)void refresh()}}>
  <summary><span class="sec-title">{$t("Scripts de test")}</span></summary>
  <div class="test-body">
    <div class="test-picker">
      <select data-test-script aria-label={$t("Scripts de test")} bind:value={selected} onchange={()=>{status='';error='';output=''}} disabled={busy||disabled||loading} title={selectedScript?.path??''}>
        <option value="">{$t("Choisir un script de test…")}</option>
        {#each scripts as script}<option value={script.id}>{script.label}</option>{/each}
      </select>
      <button class="quiet icon-btn" data-test-scripts-refresh onclick={refresh} disabled={busy||loading} title={$t("Actualiser")} aria-label={$t("Actualiser")}><Icon name="redo" size={14}/></button>
    </div>
    {#if !loading&&!scripts.length}<p class="muted">{$t("Aucun script de test trouvé.")}</p>{/if}
    {#if selectedScript}<p class="script-name" data-test-script-name title={selectedScript.path}>{selectedScript.label}</p>{/if}
    <button class="primary" data-test-script-generate onclick={generate} disabled={busy||disabled||loading||!selectedScript}>{running?$t("Script en cours…"):$t("Générer")}</button>
    {#if status}<p role="status" data-test-script-status>{status}</p>{/if}
    {#if error}<p class="error" role="alert" data-test-script-error>{error}</p>{/if}
    {#if output}<pre data-test-script-output>{output}</pre>{/if}
  </div>
</details>

<style>
  .sec{margin:0}
  .sec>summary{padding:10px 12px}
  .sec-title{font-size:13px;letter-spacing:0;text-transform:none;color:var(--text);line-height:1.5}
  .test-body{display:grid;gap:8px;padding:0 12px 12px;min-width:0}
  .test-picker{display:flex;gap:6px;min-width:0}
  select{width:100%;min-width:0;flex:1}
  button,select{min-height:32px}
  .icon-btn{flex:none}
  p{margin:0;font-size:12px;overflow-wrap:anywhere}
  .script-name{color:var(--muted)}
  pre{margin:0;max-height:180px;overflow:auto;white-space:pre-wrap;overflow-wrap:anywhere;font-size:12px;color:var(--muted)}
</style>
