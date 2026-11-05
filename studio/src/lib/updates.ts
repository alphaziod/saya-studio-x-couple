import { writable } from 'svelte/store';
import { call } from './api';

export type UpdateStatus = 'up_to_date' | 'available' | 'local_build' | 'offline';
export interface UpdateAsset { name: string; url: string; size: number }
export interface UpdateInfo { current: string; latest: string | null; status: UpdateStatus; asset: UpdateAsset | null; notesUrl: string }
export interface UpdateDownload { path: string; command: string }
export interface UpdateInstall { manual: boolean; saved?: boolean; opened?: boolean; installed?: boolean; path: string; command: string }

/** Last check result, shared by the shell banner and the settings page. */
export const updateInfo = writable<UpdateInfo | null>(null);
/** Set when the banner is closed for this session. */
export const updateDismissed = writable(false);

export async function checkForUpdate(): Promise<UpdateInfo> {
  const info = await call<UpdateInfo>('update_check');
  updateInfo.set(info);
  if (info.status === 'available') updateDismissed.set(false);
  return info;
}
export const downloadUpdate = (asset: UpdateAsset) => call<UpdateDownload>('update_download', { name: asset.name, url: asset.url });
export const installUpdate = (path: string) => call<UpdateInstall>('update_install', { path, confirmed: true });
export const openNotes = (url: string) => call('update_open_notes', { url });
