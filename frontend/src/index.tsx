/* @refresh reload */
import { render } from 'solid-js/web'
import './index.css'
import App from './App'

const root = document.getElementById('root')!
const loading = document.getElementById('app-loading')
// Old cached HTML: loading inside #root, must remove before render to avoid SolidJS DOM conflict
if (loading?.parentElement === root) loading.remove()
render(() => <App />, root)
// New HTML: loading outside #root as fixed overlay, safe to remove after render
document.getElementById('app-loading')?.remove()
