import './shim/api';
import '../../frontend/src/main';
import { openSession } from './session';

const id = document.documentElement.dataset.demoId ?? 'magazine';
void openSession(id);
