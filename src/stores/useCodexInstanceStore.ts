import * as codexInstanceService from '../services/codexInstanceService';
import { createInstanceStore } from './createInstanceStore';

export const useCodexInstanceStore = createInstanceStore(
  codexInstanceService,
  'agtools.codex.instances.cache',
);
