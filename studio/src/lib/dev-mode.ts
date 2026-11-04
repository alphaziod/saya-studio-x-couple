// Pure helpers for the developer mode page (IPC console, preference editor).
// No Svelte, no Tauri: importable from Node tests.

/** Actions that only read state. Anything not listed here asks for a confirmation before running. */
export const READ_ONLY_ACTIONS: ReadonlySet<string> = new Set([
  'preferences', 'gallery', 'gallery_window', 'gallery_neighbors', 'facets', 'detail', 'data_paths',
  'character_cards', 'character_history', 'character_jobs', 'character_sheet', 'characters',
  'casting_history', 'casting_previews', 'casting_preparation_status',
  'saya_runs', 'connection', 'models', 'model_library', 'model_detail',
  'node_catalog', 'workflows', 'workflow', 'workflow_nodes', 'jobs', 'job_details', 'job_provenance',
  'index_status', 'index_errors', 'live_state', 'memory_status', 'comfy_output_info',
  'lora_inventory', 'lora_loaders', 'lora_labels', 'library_sources',
  'scenes', 'scene', 'modules', 'module', 'pipelines', 'pipeline_runs', 'recipes', 'variants',
  'references', 'reference_history', 'semantic_status', 'visual_status', 'actions_lister',
  'action_drafts', 'checkpoints', 'generator_catalog', 'graph_sessions', 'last_queued_seed',
  'proposals', 'proposal', 'ab_campaigns', 'ab_campaign', 'sampler_steps', 'workflow_check',
]);

/** Action names called somewhere in src/ (suggestions for the console; any other name can still be typed). */
export const KNOWN_ACTIONS: readonly string[] = [
  'ab_campaign', 'ab_campaigns', 'ab_cell_diff', 'ab_plan', 'action_draft_delete', 'action_draft_promote',
  'action_drafts', 'action_draft_save', 'action_drafts_refresh', 'action_pending_refuse', 'action_pending_test',
  'action_pending_unrefuse', 'action_pending_validate', 'actions_lister', 'add_ab_parameter',
  'add_library_source', 'annotate', 'archive_character', 'associate', 'audit_prepare_profile',
  'backup_dependencies', 'backup_estimate', 'blur_document', 'blur_save', 'cancel_index',
  'cancel_pipeline_run', 'capture_checkpoint', 'casting_decide', 'casting_delete_lot',
  'casting_enregistrer_perso', 'casting_history', 'casting_lancer', 'casting_next',
  'casting_preparation_status', 'casting_previews', 'casting_publish', 'character_cards',
  'character_history', 'character_jobs', 'characters', 'character_sheet', 'checkpoint_image', 'checkpoints',
  'choose_portrait', 'civitai_download', 'civitai_search', 'civitai_thumbnail', 'comfy_output_info',
  'compare_module_versions', 'connection', 'control_job', 'couple_inspection', 'create_recipe', 'data_paths',
  'decide_proposal', 'delete_scene', 'detail', 'duplicate_scene', 'editor_export', 'editor_load',
  'engine_script', 'engine_test_script', 'engine_test_scripts', 'engine_tool', 'export_character',
  'export_prepare_profile', 'facets', 'gallery', 'gallery_cleanup', 'gallery_neighbors', 'gallery_window',
  'generate_character', 'generation_metadata', 'generation_preset', 'generator_catalog', 'graph_close',
  'graph_copy', 'graph_delete_session', 'graph_edit', 'graph_enter_subgraph', 'graph_exit_subgraph',
  'graph_export', 'graph_extract_module', 'graph_new', 'graph_open', 'graph_open_file', 'graph_paste',
  'graph_prepare_api', 'graph_resume', 'graph_save_api', 'graph_save_ui', 'graph_scoped_widget',
  'graph_sessions', 'grok_directory_files', 'grok_import', 'grok_preview', 'grok_scene', 'grok_scene_import',
  'grok_scene_preview', 'grok_scenes', 'image', 'image_temporary_close', 'image_temporary_open',
  'image_temporary_submit', 'import_character', 'import_prepare_profile', 'import_workflow', 'index',
  'index_errors', 'index_status', 'job_details', 'job_provenance', 'jobs', 'last_queued_seed',
  'library_sources', 'live_state', 'lora_cover', 'lora_inventory', 'lora_labels', 'lora_loaders',
  'lora_metadata_save', 'memory_status', 'model_detail', 'model_favorite', 'model_fetch_metadata',
  'model_import_download', 'model_library', 'model_organize_apply', 'model_organize_plan',
  'model_organize_undo', 'models', 'model_thumbnail', 'module', 'module_candidates', 'module_insert_preview',
  'modules', 'node_catalog', 'open_asset_directory', 'open_directory', 'pipeline_run', 'pipeline_runs',
  'pipelines', 'preferences', 'prepare', 'prepare_ab_cells', 'prepare_ab_grid', 'prepare_asset_trash',
  'prepare_couple', 'prepare_generated_variant', 'prepare_profile', 'prepare_profiles', 'prepare_universal',
  'preview_lora', 'preview_scene', 'proposal', 'proposals', 'recipes', 'reference_edit', 'reference_history',
  'reference_order', 'references', 'refresh_asset_presence', 'register_ab_cell_image',
  'release_pipeline_memory', 'restore_character', 'review_checkpoint', 'rollback_module', 'run_next_pass',
  'sampler_steps', 'save_ab_campaign', 'save_ab_winner_preset', 'save_bindings', 'save_character',
  'save_generated_variant', 'save_pipeline', 'save_prepare_profile', 'save_scene', 'save_semantic_binding',
  'save_variant', 'saya_resume', 'saya_runs', 'saya_start', 'saya_status', 'scene', 'scenes',
  'semantic_index', 'semantic_search', 'semantic_status', 'set_ab_verdict', 'set_catalog_removed',
  'set_library_source', 'set_preference', 'split_analysis', 'split_preview', 'split_save',
  'start_pipeline_run', 'submit', 'sync_ab_results', 'sync_jobs', 'trash_assets', 'variants',
  'verify_checkpoint', 'visual_cancel', 'visual_index', 'visual_search', 'visual_status', 'workflow',
  'workflow_nodes', 'workflows',
];

export const isReadOnlyAction = (action: string): boolean => READ_ONLY_ACTIONS.has(action.trim());

export type ArgsResult =
  | { ok: true; value: Record<string, unknown> }
  | { ok: false; error: 'invalid-json' | 'not-object' | 'reserved-key'; detail: string };

/** Parses the console arguments: empty = `{}`, otherwise a JSON object without an `action` key (serde tag). */
export function parseArgs(text: string): ArgsResult {
  const raw = text.trim();
  if (!raw) return { ok: true, value: {} };
  let parsed: unknown;
  try { parsed = JSON.parse(raw); } catch (e) { return { ok: false, error: 'invalid-json', detail: String((e as Error).message) }; }
  if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) return { ok: false, error: 'not-object', detail: '' };
  if ('action' in (parsed as object)) return { ok: false, error: 'reserved-key', detail: 'action' };
  return { ok: true, value: parsed as Record<string, unknown> };
}

export type ValueResult = { ok: true; value: unknown } | { ok: false; detail: string };

/** Parses one preference value (any JSON value, including `null`, numbers and strings). */
export function parseJsonValue(text: string): ValueResult {
  if (!text.trim()) return { ok: false, detail: 'empty' };
  try { return { ok: true, value: JSON.parse(text) }; } catch (e) { return { ok: false, detail: String((e as Error).message) }; }
}

/** Pretty JSON, cut at `max` characters so a huge result cannot freeze the page. */
export function formatJson(value: unknown, max = 100_000): string {
  let text: string;
  try { text = JSON.stringify(value, null, 2) ?? 'undefined'; } catch { text = String(value); }
  return text.length > max ? `${text.slice(0, max)}\n… (${text.length - max})` : text;
}

export interface CallEntry { id: number; action: string; args: string; ms: number; ok: boolean }

/** Newest first, bounded. */
export function pushHistory(list: CallEntry[], entry: CallEntry, limit = 20): CallEntry[] {
  return [entry, ...list].slice(0, limit);
}

/** Parent folder of a file path (`/a/b/c.db` -> `/a/b`); both separators are understood. */
export function parentDirectory(path: string): string {
  const cut = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return cut > 0 ? path.slice(0, cut) : path;
}
