import { createSignal } from 'solid-js'
import { A, useNavigate } from '@solidjs/router'
import { register } from '@/stores/auth'
import { Button, Input } from '@/components'

export default function Register() {
  const navigate = useNavigate()
  const [username, setUsername] = createSignal('')
  const [password, setPassword] = createSignal('')
  const [confirmPassword, setConfirmPassword] = createSignal('')
  const [error, setError] = createSignal('')
  const [loading, setLoading] = createSignal(false)

  const handleSubmit = async (e: Event) => {
    e.preventDefault()
    setError('')

    if (!username().trim()) {
      setError('请输入用户名')
      return
    }
    if (password().length < 6) {
      setError('密码长度至少为 6 位')
      return
    }
    if (password() !== confirmPassword()) {
      setError('两次输入的密码不一致')
      return
    }

    setLoading(true)
    try {
      await register(username().trim(), password())
      navigate('/', { replace: true })
    } catch (err: any) {
      setError(err.message || '注册失败')
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
            <h1 class="text-xl font-bold text-content">创建账号</h1>
            <p class="text-sm text-content-secondary">注册新账号以开始使用</p>
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
              placeholder="至少 6 位"
              value={password()}
              onInput={(e) => setPassword(e.currentTarget.value)}
              autocomplete="new-password"
            />
            <Input
              label="确认密码"
              type="password"
              placeholder="再次输入密码"
              value={confirmPassword()}
              onInput={(e) => setConfirmPassword(e.currentTarget.value)}
              autocomplete="new-password"
            />
            <Button
              type="submit"
              fullWidth
              size="lg"
              loading={loading()}
            >
              注册
            </Button>
          </form>

          <p class="mt-6 text-center text-sm text-content-secondary">
            已有账号？
            <A href="/login" class="text-accent hover:underline ml-1">去登录</A>
          </p>
        </div>
      </div>
    </div>
  )
}
