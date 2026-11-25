import '@fontsource/familjen-grotesk/latin-400.css';
import '@fontsource/familjen-grotesk/latin-600.css';
import '@fontsource/familjen-grotesk/latin-700.css';
import '@fontsource/fragment-mono/latin-400.css';
import '@xyflow/svelte/dist/style.css';
import '../../../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/theme.css';
import './app.css';
import { mount } from 'svelte';
import App from './App.svelte';

mount(App, { target: document.getElementById('app') });
