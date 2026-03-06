import { createSignal } from 'solid-js'
import { useNavigate } from '@solidjs/router'
import { Button, Card, CardBody, CardHeader, Input, Select, Textarea, useToast } from '@/components'
import { api, getErrorMessage } from '@/api/client'
import type { PatientReq } from '@/api/types'

export default function PatientCreate() {
  const navigate = useNavigate()
  const { toast } = useToast()

  const [name, setName] = createSignal('')
  const [gender, setGender] = createSignal('')
  const [dob, setDob] = createSignal('')
  const [phone, setPhone] = createSignal('')
  const [idNumber, setIdNumber] = createSignal('')
  const [notes, setNotes] = createSignal('')

  const [errors, setErrors] = createSignal<Record<string, string>>({})
  const [submitting, setSubmitting] = createSignal(false)

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
      const patient = await api.patients.create(data)
      toast('success', '患者创建成功')
      navigate(`/patients/${patient.id}`)
    } catch (err: unknown) {
      toast('error', getErrorMessage(err) || '创建失败')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div class="form-page page-shell">
      <Card variant="elevated">
        <CardHeader>
          <h1 class="form-title">新建患者</h1>
        </CardHeader>
        <CardBody>
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
                label="联系电话（选填）"
                placeholder="请输入联系电话"
                value={phone()}
                onInput={(e) => setPhone(e.currentTarget.value)}
                error={errors().phone}
              />
            </div>

            <div class={errors().id_number ? 'animate-shake' : ''}>
              <Input
                label="身份证号（选填）"
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
                onClick={() => navigate('/')}
              >
                取消
              </Button>
              <Button type="submit" loading={submitting()}>
                创建患者
              </Button>
            </div>
          </form>
        </CardBody>
      </Card>
    </div>
  )
}
