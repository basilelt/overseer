import type { DaemonClient } from '../daemon/client';
import { t } from '../i18n.svelte';
import { requirePath, savePasted } from '../kanban/task-attachments';
import { savePasteAll, uploadFailures, uploadList } from '../upload-pipeline';
import { fileInfos, planPaste, refusalMessage, uploadedPath } from '../uploads';

export interface ComposeAttachDeps {
  client: Pick<DaemonClient, 'savePaste' | 'uploadFile'>;
  clipboardImage: () => Promise<string | null>;
  notify: (text: string, kind?: 'info' | 'error', action?: undefined, entry?: 'upload') => void;
}

const message = (e: unknown) => String((e as Error)?.message ?? e);

export async function attachImages(files: File[], deps: ComposeAttachDeps): Promise<string[]> {
  const { client, notify } = deps;
  let paths: string[];
  if (files.length) {
    const plan = planPaste({ text: '', files: fileInfos(files) }, true);
    if (plan.kind === 'reject') {
      notify(refusalMessage(plan), 'error');
      return [];
    }
    const saved = await savePasteAll(files, (name, bytes) => savePasted(client, name, bytes), (name, e) => notify(t('upload.pasteFailed', { name, error: message(e) }), 'error'));
    if (!saved) return [];
    paths = saved;
  } else {
    const clip = await deps.clipboardImage();
    paths = clip ? [clip] : [];
  }
  const failures = uploadFailures();
  const done = await uploadList((p, onProgress) => client.uploadFile(p, onProgress), paths, {
    progress: () => {},
    validate: (r) => void requirePath(r?.path),
    fail: failures.fail,
  });
  const failure = failures.summary();
  if (failure) notify(failure, 'error', undefined, 'upload');
  return done.map(uploadedPath);
}
