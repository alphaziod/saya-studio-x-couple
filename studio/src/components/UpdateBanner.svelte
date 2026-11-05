<script lang="ts">
  import { t } from '../lib/i18n';
  import { message } from '../lib/api';
  import { updateInfo, updateDismissed, downloadUpdate, installUpdate, openNotes, type UpdateDownload } from '../lib/updates';
  import Icon from './Icon.svelte';
  let busy = $state(false), error = $state(''), note = $state('');
  let ready = $state<UpdateDownload | null>(null), manual = $state('');
  const info = $derived($updateInfo);
  const shown = $derived(info?.status === 'available' && !$updateDismissed);
  async function update() {
    if (busy || !info?.asset) return; busy = true; error = ''; note = '';
    try { ready = await downloadUpdate(info.asset); } catch (e) { error = message(e); } finally { busy = false; }
  }
  async function install() {
    if (busy || !ready) return; busy = true; error = '';
    const file = ready;
    try {
      const done = await installUpdate(file.path);
      ready = null;
      if (done.installed) note = $t("Mise à jour installée. Relance Saya Studio.");
      else if (done.opened) note = $t("Installateur ouvert.");
      else if (done.saved) note = $t("Mise à jour enregistrée ici : {path}", { path: done.path });
      else manual = done.command;
    } catch (e) { error = message(e); } finally { busy = false; }
  }
  async function copy() {
    try { await navigator.clipboard.writeText(manual); note = $t("Commande copiée."); } catch { /* the command stays visible */ }
  }
  async function notes() {
    if (info) try { await openNotes(info.notesUrl); } catch (e) { error = message(e); }
  }
</script>
{#if shown && info}
  <div class="update-banner notice" role="status" data-update-banner>
    <div class="update-row">
      <strong>{$t("Saya Studio {version} est disponible", { version: info.latest ?? '' })}</strong>
      {#if !ready && !manual}
        {#if info.asset}<button class="primary" data-update-start disabled={busy} onclick={update}>{busy ? $t("Téléchargement de la mise à jour…") : $t("Mettre à jour")}</button>
        {:else}<span class="subtle">{$t("Aucun paquet adapté à ce système n’est disponible dans cette version.")}</span>{/if}
        <button data-update-notes onclick={notes}>{$t("Notes")}</button>
      {/if}
      <button class="quiet icon-btn" aria-label={$t("Fermer")} onclick={() => updateDismissed.set(true)}><Icon name="close"/></button>
    </div>
    {#if ready}
      <div class="update-confirm" role="alertdialog" aria-label={$t("Installer la mise à jour ?")} data-update-confirm>
        <strong>{$t("Installer la mise à jour ?")}</strong>
        <span class="subtle">{$t("Commande qui sera exécutée :")}</span>
        <code data-update-command>{ready.command}</code>
        <div class="update-row">
          <button class="primary" disabled={busy} onclick={install}>{$t("Installer")}</button>
          <button disabled={busy} onclick={() => { ready = null; }}>{$t("Annuler")}</button>
        </div>
      </div>
    {/if}
    {#if manual}
      <div class="update-confirm" data-update-manual>
        <span class="subtle">{$t("Mise à jour téléchargée et vérifiée. Lance cette commande dans un terminal :")}</span>
        <code>{manual}</code>
        <div class="update-row"><button onclick={copy}>{$t("Copier la commande")}</button></div>
      </div>
    {/if}
    {#if note}<p class="subtle" role="status">{note}</p>{/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </div>
{/if}
<style>
  .update-banner{display:grid;gap:8px;margin-bottom:12px}
  .update-row{display:flex;flex-wrap:wrap;align-items:center;gap:10px}
  .update-confirm{display:grid;gap:6px}
  .update-confirm code{font-size:12px;overflow-wrap:anywhere;background:var(--canvas);padding:6px 8px;border-radius:5px}
  p{margin:0}
</style>
