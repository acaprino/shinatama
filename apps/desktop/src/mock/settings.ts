// The answers of the browser for the commands of this area (see mock/index.ts). The agent that owns the area fills them in.
import type { Answers } from './index.ts';

export const answers: Answers = {
  presentation_load: { revision: 1, settings: { version: 1, skin: 'shinatama', followSkin: false, fontFamily: 'profile', fontSize: 13, lineHeight: 115 } },
  presentation_save: (args) => ({ revision: 2, settings: (args?.request as { settings: unknown }).settings }),
  storage_location: { directory: 'C:\\Users\\me\\AppData\\Roaming\\Shinatama', database_path: 'C:\\Users\\me\\AppData\\Roaming\\Shinatama\\shinatama.db', read_only: false, message: null },
  attention_configure: {},
  maintenance_activity: null,
  maintenance_status: { phase: 'idle', completed_at: null, message: null },
};
