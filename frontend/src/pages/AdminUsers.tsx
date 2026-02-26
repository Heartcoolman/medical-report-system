import { createSignal, createResource, Show, For } from 'solid-js'
import { api } from '@/api/client'
import type { UserInfo } from '@/api/types'
import { Button, Card, CardBody, Badge, Modal, Select, useToast, Spinner, Empty } from '@/components'

const ROLE_LABELS: Record<string, string> = {
  admin: '管理员',
  doctor: '医生',
  nurse: '护士',
  readonly: '只读',
}

const ROLE_VARIANTS: Record<string, 'accent' | 'info' | 'warning' | 'success' | 'error'> = {
  admin: 'error',
  doctor: 'accent',
  nurse: 'info',
  readonly: 'warning',
}

export default function AdminUsers() {
  const { toast } = useToast()
  const [users, { refetch }] = createResource(() => api.admin.listUsers())
  const [editingUser, setEditingUser] = createSignal<UserInfo | null>(null)
  const [newRole, setNewRole] = createSignal('')
  const [saving, setSaving] = createSignal(false)
  const [deleteUser, setDeleteUser] = createSignal<UserInfo | null>(null)
  const [deleting, setDeleting] = createSignal(false)

  async function handleUpdateRole() {
    const user = editingUser()
    if (!user || !newRole()) return
    setSaving(true)
    try {
      await api.admin.updateUserRole(user.id, newRole())
      toast('success', `${user.username} 的角色已更新为 ${ROLE_LABELS[newRole()] || newRole()}`)
      setEditingUser(null)
      refetch()
    } catch (err: any) {
      toast('error', err.message || '更新失败')
    } finally {
      setSaving(false)
    }
  }

  async function handleDelete() {
    const user = deleteUser()
    if (!user) return
    setDeleting(true)
    try {
      await api.admin.deleteUser(user.id)
      toast('success', `用户 ${user.username} 已删除`)
      setDeleteUser(null)
      refetch()
    } catch (err: any) {
      toast('error', err.message || '删除失败')
    } finally {
      setDeleting(false)
    }
  }

  return (
    <div class="page-shell">
      <div class="max-w-3xl mx-auto">
        <h1 class="page-title mb-6">用户管理</h1>

        <Show when={users.loading}>
          <div class="flex justify-center py-12">
            <Spinner size="lg" variant="orbital" />
          </div>
        </Show>

        <Show when={users.error}>
          <Empty title="加载失败" description={String(users.error?.message || users.error)} />
        </Show>

        <Show when={users() && !users.loading}>
          <div class="space-y-3">
            <For each={users()}>
              {(user) => (
                <Card variant="outlined">
                  <CardBody class="p-4 flex items-center justify-between">
                    <div class="flex items-center gap-3">
                      <div class="w-10 h-10 rounded-full bg-surface-secondary flex items-center justify-center text-sm font-semibold text-content">
                        {user.username.slice(0, 1).toUpperCase()}
                      </div>
                      <div>
                        <div class="text-sm font-semibold text-content">{user.username}</div>
                        <div class="text-xs text-content-tertiary">
                          创建于 {user.created_at?.slice(0, 10) || '未知'}
                        </div>
                      </div>
                    </div>
                    <div class="flex items-center gap-2">
                      <Badge variant={ROLE_VARIANTS[user.role] || 'warning'}>
                        {ROLE_LABELS[user.role] || user.role}
                      </Badge>
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => {
                          setEditingUser(user)
                          setNewRole(user.role)
                        }}
                      >
                        修改角色
                      </Button>
                      <Button
                        variant="danger"
                        size="sm"
                        onClick={() => setDeleteUser(user)}
                      >
                        删除
                      </Button>
                    </div>
                  </CardBody>
                </Card>
              )}
            </For>
          </div>
        </Show>

        {/* Edit Role Modal */}
        <Modal
          open={!!editingUser()}
          onClose={() => setEditingUser(null)}
          title={`修改 ${editingUser()?.username} 的角色`}
        >
          <div class="space-y-4">
            <Select
              value={newRole()}
              onChange={(e) => setNewRole(e.currentTarget.value)}
            >
              <option value="admin">管理员</option>
              <option value="doctor">医生</option>
              <option value="nurse">护士</option>
              <option value="readonly">只读</option>
            </Select>
            <div class="flex gap-2 justify-end">
              <Button variant="outline" onClick={() => setEditingUser(null)}>取消</Button>
              <Button variant="primary" loading={saving()} onClick={handleUpdateRole}>确认</Button>
            </div>
          </div>
        </Modal>

        {/* Delete Confirm Modal */}
        <Modal
          open={!!deleteUser()}
          onClose={() => setDeleteUser(null)}
          title="确认删除用户"
        >
          <p class="text-sm text-content-secondary mb-4">
            确定要删除用户 <strong>{deleteUser()?.username}</strong> 吗？此操作不可恢复。
          </p>
          <div class="flex gap-2 justify-end">
            <Button variant="outline" onClick={() => setDeleteUser(null)}>取消</Button>
            <Button variant="danger" loading={deleting()} onClick={handleDelete}>确认删除</Button>
          </div>
        </Modal>
      </div>
    </div>
  )
}
