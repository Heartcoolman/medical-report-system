import { createSignal, onMount, Show } from 'solid-js'
import { api } from '@/api/client'
import { useToast, Button, Spinner } from '@/components'

export default function Settings() {
  const { toast } = useToast()
  const [loading, setLoading] = createSignal(true)
  const [saving, setSaving] = createSignal(false)
  const [llmKey, setLlmKey] = createSignal('')
  const [interpretKey, setInterpretKey] = createSignal('')
  const [siliconflowKey, setSiliconflowKey] = createSignal('')
  const [showLlm, setShowLlm] = createSignal(false)
  const [showInterpret, setShowInterpret] = createSignal(false)
  const [showSiliconflow, setShowSiliconflow] = createSignal(false)

  onMount(async () => {
    try {
      const settings = await api.user.getSettings()
      setLlmKey(settings.llm_api_key || '')
      setInterpretKey(settings.interpret_api_key || '')
      setSiliconflowKey(settings.siliconflow_api_key || '')
    } catch (err: any) {
      toast('error', err.message || '加载设置失败')
    } finally {
      setLoading(false)
    }
  })

  const handleSave = async () => {
    setSaving(true)
    try {
      await api.user.updateSettings({
        llm_api_key: llmKey(),
        interpret_api_key: interpretKey(),
        siliconflow_api_key: siliconflowKey(),
      })
      toast('success', '设置已保存')
    } catch (err: any) {
      toast('error', err.message || '保存失败')
    } finally {
      setSaving(false)
    }
  }

  const EyeIcon = () => (
    <svg class="w-4.5 h-4.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
      <path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
      <path stroke-linecap="round" stroke-linejoin="round" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
    </svg>
  )

  const EyeOffIcon = () => (
    <svg class="w-4.5 h-4.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
      <path stroke-linecap="round" stroke-linejoin="round" d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21" />
    </svg>
  )

  return (
    <div class="page-shell">
      <div class="mx-auto max-w-lg">
        <h1 class="page-title mb-6">用户设置</h1>

        <Show
          when={!loading()}
          fallback={
            <div class="flex justify-center py-12">
              <Spinner size="lg" variant="orbital" />
            </div>
          }
        >
          <div class="bg-surface-elevated rounded-2xl shadow-lg border border-border/40 p-6">
            <p class="text-sm text-content-secondary mb-6">
              配置您自己的 API 密钥，未配置时将使用系统默认密钥
            </p>

            <div class="flex flex-col gap-5">
              <div class="flex flex-col gap-1.5">
                <label class="form-label">LLM API Key（通义千问）</label>
                <div class="relative">
                  <input
                    type={showLlm() ? 'text' : 'password'}
                    value={llmKey()}
                    onInput={(e) => setLlmKey(e.currentTarget.value)}
                    placeholder="输入 API Key"
                    class="form-control-base form-control-input pr-10"
                  />
                  <button
                    type="button"
                    class="absolute right-2 top-1/2 -translate-y-1/2 p-1 rounded-lg text-content-tertiary hover:text-content transition-colors cursor-pointer"
                    onClick={() => setShowLlm(!showLlm())}
                    aria-label={showLlm() ? '隐藏' : '显示'}
                  >
                    <Show when={showLlm()} fallback={<EyeIcon />}>
                      <EyeOffIcon />
                    </Show>
                  </button>
                </div>
              </div>

              <div class="flex flex-col gap-1.5">
                <label class="form-label">Interpret API Key（AI 解读）</label>
                <div class="relative">
                  <input
                    type={showInterpret() ? 'text' : 'password'}
                    value={interpretKey()}
                    onInput={(e) => setInterpretKey(e.currentTarget.value)}
                    placeholder="输入 API Key"
                    class="form-control-base form-control-input pr-10"
                  />
                  <button
                    type="button"
                    class="absolute right-2 top-1/2 -translate-y-1/2 p-1 rounded-lg text-content-tertiary hover:text-content transition-colors cursor-pointer"
                    onClick={() => setShowInterpret(!showInterpret())}
                    aria-label={showInterpret() ? '隐藏' : '显示'}
                  >
                    <Show when={showInterpret()} fallback={<EyeIcon />}>
                      <EyeOffIcon />
                    </Show>
                  </button>
                </div>
              </div>

              <div class="flex flex-col gap-1.5">
                <label class="form-label">SiliconFlow API Key（视觉识别）</label>
                <div class="relative">
                  <input
                    type={showSiliconflow() ? 'text' : 'password'}
                    value={siliconflowKey()}
                    onInput={(e) => setSiliconflowKey(e.currentTarget.value)}
                    placeholder="输入 API Key"
                    class="form-control-base form-control-input pr-10"
                  />
                  <button
                    type="button"
                    class="absolute right-2 top-1/2 -translate-y-1/2 p-1 rounded-lg text-content-tertiary hover:text-content transition-colors cursor-pointer"
                    onClick={() => setShowSiliconflow(!showSiliconflow())}
                    aria-label={showSiliconflow() ? '隐藏' : '显示'}
                  >
                    <Show when={showSiliconflow()} fallback={<EyeIcon />}>
                      <EyeOffIcon />
                    </Show>
                  </button>
                </div>
              </div>
            </div>

            <div class="mt-6">
              <Button
                onClick={handleSave}
                loading={saving()}
                size="lg"
                fullWidth
              >
                保存设置
              </Button>
            </div>
          </div>
        </Show>
      </div>
    </div>
  )
}
