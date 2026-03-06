import { createSignal } from 'solid-js'
import { A, useNavigate } from '@solidjs/router'
import { getErrorMessage } from '@/api/client'
import { login } from '@/stores/auth'
import { Button, Input } from '@/components'

export default function Login() {
  const navigate = useNavigate()
  const [username, setUsername] = createSignal('')
  const [password, setPassword] = createSignal('')
  const [error, setError] = createSignal('')
  const [loading, setLoading] = createSignal(false)

  const handleSubmit = async (e: Event) => {
    e.preventDefault()
    setError('')
    if (!username().trim() || !password()) {
      setError('请输入用户名和密码')
      return
    }
    setLoading(true)
    try {
      await login(username().trim(), password())
      navigate('/', { replace: true })
    } catch (err: unknown) {
      setError(getErrorMessage(err) || '登录失败')
    } finally {
      setLoading(false)
    }
  }

  return (
    <div class="min-h-screen flex items-center justify-center bg-surface px-4">
      <div class="w-full max-w-sm">
        <div class="bg-surface-elevated rounded-2xl shadow-lg border border-border/40 p-8">
          <div class="flex flex-col items-center gap-2 mb-8">
            <svg class="w-10 h-10 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
            </svg>
            <h1 class="text-xl font-bold text-content">医疗报告管理</h1>
            <p class="text-sm text-content-secondary">登录您的账号</p>
          </div>

          {error() && (
            <div class="mb-4 px-4 py-3 rounded-xl bg-error-light text-error text-sm">
              {error()}
            </div>
          )}

          <form onSubmit={handleSubmit} class="flex flex-col gap-4">
            <Input
              label="用户名"
              placeholder="请输入用户名"
              value={username()}
              onInput={(e) => setUsername(e.currentTarget.value)}
              autocomplete="username"
            />
            <Input
              label="密码"
              type="password"
              placeholder="请输入密码"
              value={password()}
              onInput={(e) => setPassword(e.currentTarget.value)}
              autocomplete="current-password"
            />
            <Button
              type="submit"
              fullWidth
              size="lg"
              loading={loading()}
            >
              登录
            </Button>
          </form>

          <p class="mt-6 text-center text-sm text-content-secondary">
            没有账号？
            <A href="/register" class="text-accent hover:underline ml-1">去注册</A>
          </p>
        </div>
      </div>
    </div>
  )
}
