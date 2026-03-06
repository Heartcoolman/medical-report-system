import { createSignal, createResource, Show } from 'solid-js'
import { useNavigate, useParams } from '@solidjs/router'
import { Button, Card, CardBody, CardHeader, Input, Select, Skeleton, Textarea, useToast } from '@/components'
import { api, getErrorMessage } from '@/api/client'
import type { PatientReq } from '@/api/types'

export default function PatientEdit() {
  const params = useParams<{ id: string }>()
  const navigate = useNavigate()
  const { toast } = useToast()

  const [patient] = createResource(() => params.id, (id) => api.patients.get(id))

  const [name, setName] = createSignal('')
  const [gender, setGender] = createSignal('')
  const [dob, setDob] = createSignal('')
  const [phone, setPhone] = createSignal('')
  const [idNumber, setIdNumber] = createSignal('')
  const [notes, setNotes] = createSignal('')
  const [initialized, setInitialized] = createSignal(false)

  const [errors, setErrors] = createSignal<Record<string, string>>({})
  const [submitting, setSubmitting] = createSignal(false)

  // Populate form when patient data loads
  const populateForm = () => {
    const p = patient()
    if (p && !initialized()) {
      setName(p.name)
      setGender(p.gender)
      setDob(p.dob || '')
      setPhone(p.phone)
      setIdNumber(p.id_number)
      setNotes(p.notes || '')
      setInitialized(true)
    }
  }

  function validate(): boolean {
    const errs: Record<string, string> = {}
    if (!name().trim()) errs.name = '请输入患者姓名'
    if (!gender()) errs.gender = '请选择性别'
    setErrors(errs)
    return Object.keys(errs).length === 0
  }

  async function handleSubmit(e: Event) {
    e.preventDefault()
    if (!validate()) return

    setSubmitting(true)
    try {
      const data: PatientReq = {
        name: name().trim(),
        gender: gender() as '男' | '女',
        dob: dob() || undefined,
        phone: phone().trim(),
        id_number: idNumber().trim(),
        notes: notes().trim() || undefined,
      }
      await api.patients.update(params.id, data)
      toast('success', '患者信息已更新')
      navigate(`/patients/${params.id}`)
    } catch (err: unknown) {
      toast('error', getErrorMessage(err) || '更新失败')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div class="form-page page-shell">
      <Card variant="elevated">
        <CardHeader>
        <h1 class="form-title">编辑患者</h1>
        </CardHeader>
        <CardBody>
          <Show when={patient.loading}>
            <div class="flex flex-col gap-4">
              <Skeleton variant="text" height={40} />
              <Skeleton variant="text" height={40} />
              <Skeleton variant="text" height={40} />
              <Skeleton variant="text" height={40} />
              <Skeleton variant="text" height={40} />
              <Skeleton variant="rect" height={80} />
            </div>
          </Show>

          <Show when={patient.error}>
            <div class="text-center py-8">
              <p class="text-error">加载患者信息失败</p>
              <Button variant="outline" class="mt-4" onClick={() => navigate('/')}>
                返回首页
              </Button>
            </div>
          </Show>

          <Show when={patient()}>
            {(() => {
              populateForm()
              return null
            })()}
            <form onSubmit={handleSubmit} class="flex flex-col gap-4">
              <div class={errors().name ? 'animate-shake' : ''}>
                <Input
                  label="姓名"
                  placeholder="请输入患者姓名"
                  value={name()}
                  onInput={(e) => setName(e.currentTarget.value)}
                  error={errors().name}
                />
              </div>

              <div class={errors().gender ? 'animate-shake' : ''}>
                <Select
                  label="性别"
                  value={gender()}
                  onChange={(e) => setGender(e.currentTarget.value)}
                  error={errors().gender}
                >
                  <option value="">请选择性别</option>
                  <option value="男">男</option>
                  <option value="女">女</option>
                </Select>
              </div>

              <Input
                label="出生日期"
                type="date"
                value={dob()}
                onInput={(e) => setDob(e.currentTarget.value)}
              />

              <div class={errors().phone ? 'animate-shake' : ''}>
                <Input
                  label="联系电话"
                  placeholder="请输入联系电话"
                  value={phone()}
                  onInput={(e) => setPhone(e.currentTarget.value)}
                  error={errors().phone}
                />
              </div>

              <div class={errors().id_number ? 'animate-shake' : ''}>
                <Input
                  label="身份证号"
                  placeholder="请输入身份证号"
                  value={idNumber()}
                  onInput={(e) => setIdNumber(e.currentTarget.value)}
                  error={errors().id_number}
                />
              </div>

              <Textarea
                label="备注"
                placeholder="可选备注信息"
                value={notes()}
                onInput={(e) => setNotes(e.currentTarget.value)}
                rows={3}
              />

              <div class="flex justify-end gap-3 pt-2">
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => navigate(`/patients/${params.id}`)}
                >
                  取消
                </Button>
                <Button type="submit" loading={submitting()}>
                  保存修改
                </Button>
              </div>
            </form>
          </Show>
        </CardBody>
      </Card>
    </div>
  )
}
