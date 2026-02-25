/* @refresh reload */
import { render } from 'solid-js/web'
import './index.css'
import App from './App'

const root = document.getElementById('root')!
document.getElementById('app-loading')?.remove()
render(() => <App />, root)
