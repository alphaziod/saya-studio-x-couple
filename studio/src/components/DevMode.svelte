<script lang="ts">
  import { get } from 'svelte/store';
  import { onMount } from 'svelte';
  import { call, message, native } from '../lib/api';
  import { invoke } from '@tauri-apps/api/core';
  import { t } from '../lib/i18n';
  import { KNOWN_ACTIONS, formatJson, isReadOnlyAction, parentDirectory, parseArgs, parseJsonValue, pushHistory, type CallEntry } from '../lib/dev-mode';
  // `onprefs` lets the shell react when the preference editor changes devMode itself.
  let { onprefs }: { onprefs?: (prefs: Record<string, unknown>) => void } = $props();

  let error = $state(''), note = $state('');

  // Workflow check: the node classes a workflow needs, against the running ComfyUI (read-only).
  let checkBusy = $state(false), checkReport = $state<Record<string, unknown> | null>(null);
  async function checkWorkflow() {
    if (checkBusy) return;
    checkBusy = true; error = ''; checkReport = null;
    try {
      const path = await invoke<string | null>('pick_workflow_file');
      if (path) checkReport = await call<Record<string, unknown>>('workflow_check', { path });
    } catch (e) { error = message(e); } finally { checkBusy = false; }
  }
  const missingClasses = $derived(((checkReport?.missing as { class: string }[] | undefined) ?? []).map(m => m.class));

  // a. IPC console. Every call goes through call(); non-read actions are confirmed first.
  let action = $state(''), argsText = $state('{}'), running = $state(false), result = $state(''), resultFailed = $state(false);
  let history = $state<CallEntry[]>([]), nextId = 1;
  async function run() {
    if (running) return;
    const name = action.trim();
    if (!name) return;
    const parsed = parseArgs(argsText);
    if (!parsed.ok) {
      resultFailed = true;
      result = parsed.error === 'invalid-json' ? get(t)("JSON invalide : {detail}", { detail: parsed.detail })
        : parsed.error === 'not-object' ? get(t)("Les arguments doivent être un objet JSON.")
        : get(t)("La clé « action » est réservée : elle porte le nom de l’action.");
      return;
    }
    if (!isReadOnlyAction(name) && !window.confirm(get(t)("Exécuter l’action « {action} » ? Elle peut modifier des données.", { action: name }))) return;
    running = true; resultFailed = false; result = '';
    const start = performance.now();
    let ok = true;
    try { result = formatJson(await call(name, parsed.value)); }
    catch (e) { ok = false; resultFailed = true; result = message(e); }
    finally {
      running = false;
      history = pushHistory(history, { id: nextId++, action: name, args: argsText, ms: Math.round(performance.now() - start), ok });
    }
  }
  function reuse(entry: CallEntry) { action = entry.action; argsText = entry.args; }

  // b. Preferences editor.
  let prefs = $state<Record<string, unknown>>({}), prefKey = $state(''), prefDraft = $state(''), prefBusy = $state(false);
  async function loadPrefs() {
    try { prefs = await call<Record<string, unknown>>('preferences'); onprefs?.(prefs); }
    catch (e) { error = message(e); }
  }
  function pickKey(key: string) {
    prefKey = key;
    prefDraft = key in prefs ? formatJson(prefs[key]) : '';
  }
  async function savePref() {
    if (prefBusy) return;
    const key = prefKey.trim();
    if (!key) return;
    error = ''; note = '';
    const parsed = parseJsonValue(prefDraft);
    if (!parsed.ok) { error = get(t)("JSON invalide : {detail}", { detail: parsed.detail }); return; }
    if (!window.confirm(get(t)("Enregistrer la préférence « {key} » ?", { key }))) return;
    prefBusy = true;
    try { await call('set_preference', { key, value: parsed.value }); note = get(t)("Préférence « {key} » enregistrée.", { key }); await loadPrefs(); pickKey(key); }
    catch (e) { error = message(e); } finally { prefBusy = false; }
  }

  // c. Data paths and folders.
  let paths = $state<Record<string, unknown> | null>(null), openBusy = $state(false);
  const folderPrefs = ['captureRoot', 'libraryRoot', 'loraRoot', 'comfyOutputDir'];
  const folders = $derived.by(() => {
    const out: { label: string; path: string }[] = [];
    if (typeof paths?.database === 'string') out.push({ label: get(t)("Dossier des données Studio"), path: parentDirectory(paths.database) });
    for (const key of folderPrefs) { const value = prefs[key]; if (typeof value === 'string' && value.trim()) out.push({ label: key, path: value }); }
    return out;
  });
  async function openFolder(path: string) {
    if (openBusy) return;
    openBusy = true; error = '';
    try { await call('open_directory', { path }); } catch (e) { error = message(e); } finally { openBusy = false; }
  }

  onMount(() => {
    if (!native) return;
    void loadPrefs();
    call<Record<string, unknown>>('data_paths').then(value => { paths = value; }).catch(e => { error = message(e); });
  });
</script>
<div class="dev-page" data-dev-mode>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if note}<p class="notice" role="status">{note}</p>{/if}

  <fieldset class="dsec" disabled={!native}>
    <legend class="dtitle">{$t("Console IPC")}</legend>
    <label class="frow"><span>{$t("Action")}</span><input list="dev-actions" data-dev-action bind:value={action} spellcheck="false" autocomplete="off" disabled={running}/></label>
    <datalist id="dev-actions">{#each KNOWN_ACTIONS as name}<option value={name}></option>{/each}</datalist>
    <label class="frow top"><span>{$t("Arguments (JSON)")}</span><textarea data-dev-args rows="5" bind:value={argsText} spellcheck="false" disabled={running}></textarea></label>
    <div class="actions"><button class="primary" data-dev-run disabled={running || !action.trim()} onclick={run}>{$t("Exécuter")}</button><span class="muted">{$t("Les actions hors lecture demandent une confirmation.")}</span></div>
    {#if result}<pre data-dev-result class:error={resultFailed}>{result}</pre>{/if}
    {#if history.length}
      <ul class="hist" data-dev-history>
        {#each history as entry (entry.id)}<li><button class="quiet" onclick={() => reuse(entry)} title={entry.args}><code>{entry.action}</code></button><span class="num">{entry.ms} ms</span><span class:error={!entry.ok}>{entry.ok ? $t("OK") : $t("Erreur")}</span></li>{/each}
      </ul>
    {/if}
  </fieldset>

  <fieldset class="dsec" disabled={!native}>
    <legend class="dtitle">{$t("Préférences")}</legend>
    <label class="frow"><span>{$t("Clé")}</span><input list="dev-pref-keys" data-dev-pref-key value={prefKey} oninput={e => pickKey(e.currentTarget.value)} spellcheck="false" autocomplete="off" disabled={prefBusy}/></label>
    <datalist id="dev-pref-keys">{#each Object.keys(prefs).sort() as key}<option value={key}></option>{/each}</datalist>
    <label class="frow top"><span>{$t("Valeur (JSON)")}</span><textarea data-dev-pref-value rows="4" bind:value={prefDraft} spellcheck="false" disabled={prefBusy}></textarea></label>
    <div class="actions"><button disabled={prefBusy || !prefKey.trim()} onclick={savePref}>{$t("Enregistrer")}</button><button class="quiet" disabled={prefBusy} onclick={loadPrefs}>{$t("Recharger")}</button></div>
    <pre data-dev-prefs>{formatJson(prefs)}</pre>
  </fieldset>

  <fieldset class="dsec" disabled={!native}>
    <legend class="dtitle">{$t("Chemins des données")}</legend>
    {#if paths}<pre data-dev-paths>{formatJson(paths)}</pre>{/if}
    <div class="actions">{#each folders as folder}<button disabled={openBusy} title={folder.path} onclick={() => openFolder(folder.path)}>{$t("Ouvrir : {label}", { label: folder.label })}</button>{/each}</div>
  </fieldset>

  <section class="dsec" data-dev-workflow-check>
    <h2 class="dtitle">{$t("Vérifier un workflow")}</h2>
    <p class="muted">{$t("Liste les nodes dont le workflow a besoin et ceux qui manquent dans le ComfyUI en marche. Aucun fichier n'est modifié.")}</p>
    <div class="actions"><button disabled={checkBusy || !native} onclick={checkWorkflow}>{checkBusy ? $t("Vérification…") : $t("Choisir un workflow…")}</button></div>
    {#if checkReport}
      <p data-dev-check-verdict class:error={!checkReport.complete}>{checkReport.complete ? $t("Complet : tous les nodes sont présents.") : missingClasses.length ? $t("Nodes manquants : {list}", { list: missingClasses.join(', ') }) : $t("Incomplet : voir les entrées non résolues ci-dessous.")}</p>
      <pre data-dev-check-report>{formatJson(checkReport)}</pre>
    {/if}
  </section>

  <section class="dsec">
    <h2 class="dtitle">{$t("Trace ComfyUI")}</h2>
    <p class="muted">{$t("Pour tracer les échanges avec ComfyUI, lancez Saya Studio avec la variable SAYA_STUDIO_TRACE=1.")}</p>
  </section>
</div>
<style>
  .dev-page{max-width:760px;display:grid}
  p{margin:0}
  .notice{background:none;border:0;padding:6px 0;color:var(--ok)}
  .error{padding:6px 0}
  .dsec{border:0;border-bottom:1px solid var(--line);margin:0;padding:20px 0;display:grid;gap:12px;min-width:0}
  .dtitle{float:left;width:100%;padding:0;margin:0 0 4px;font-size:14px;font-weight:600;letter-spacing:normal;text-transform:none;color:var(--muted)}
  .dtitle+*{clear:both}
  .frow{display:grid;grid-template-columns:200px minmax(0,1fr);align-items:center;gap:12px;flex-direction:row;color:var(--text);font-size:12.5px;margin:0}
  .frow.top{align-items:start}
  .frow>span{color:var(--muted)}
  .frow input,.frow textarea{width:100%;font-family:ui-monospace,monospace;font-size:12px}
  .actions{display:flex;flex-wrap:wrap;align-items:center;gap:8px;margin-inline-start:212px}
  pre{background:var(--canvas);border:1px solid var(--line);border-radius:var(--radius);padding:8px;max-height:320px;margin:0}
  pre.error{color:var(--danger)}
  .hist{margin:0;padding:0;list-style:none;font-size:12px}
  .hist li{display:flex;align-items:center;gap:12px;padding:3px 0;border-top:1px solid var(--line)}
  .hist li button{padding:2px 6px}
  .hist .num{color:var(--muted)}
  @media(max-width:700px){.frow{grid-template-columns:1fr;gap:4px}.actions{margin-inline-start:0}}
</style>
