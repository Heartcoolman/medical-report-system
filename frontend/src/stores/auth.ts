import { createSignal } from 'solid-js'
import { api } from '@/api/client'

export interface AuthUser {
  id: string
  username: string
  role: string
}

const TOKEN_KEY = 'auth_token'
const REFRESH_TOKEN_KEY = 'refresh_token'

const [user, setUser] = createSignal<AuthUser | null>(null)
const [ready, setReady] = createSignal(false)

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY)
}

export function setToken(token: string | null) {
  if (token) {
    localStorage.setItem(TOKEN_KEY, token)
  } else {
    localStorage.removeItem(TOKEN_KEY)
  }
}

export function isAuthenticated() {
  return !!getToken()
}

export function currentUser() {
  return user()
}

export function authReady() {
  return ready()
}

export async function login(username: string, password: string) {
  const res = await api.auth.login(username, password)
  // api.auth.login already stores tokens in localStorage via .then()
  setUser(res.user)
  return res
}

export async function register(username: string, password: string) {
  const res = await api.auth.register(username, password)
  // api.auth.register already stores tokens in localStorage via .then()
  setUser(res.user)
  return res
}

export function logout() {
  api.auth.logout() // revoke refresh token server-side (fire-and-forget)
  setUser(null)
}

export async function initAuth() {
  const token = getToken()
  if (!token) {
    setReady(true)
    return
  }
  try {
    const me = await api.auth.me()
    setUser(me)
  } catch {
    setToken(null)
    localStorage.removeItem(REFRESH_TOKEN_KEY)
    setUser(null)
  } finally {
    setReady(true)
  }
}
