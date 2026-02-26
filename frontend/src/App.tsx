import { lazy, Suspense, Show, createEffect, type ParentProps } from 'solid-js'
import { ToastProvider, Spinner } from './components'
import { Router, Route, A, useNavigate } from '@solidjs/router'
import AppLayout from './layouts/AppLayout'
import ReloadPrompt from './components/ReloadPrompt'
import PWAInstallPrompt from './components/PWAInstallPrompt'
import { isAuthenticated, initAuth, authReady } from './stores/auth'

const Dashboard = lazy(() => import('./pages/Dashboard'))
const PatientCreate = lazy(() => import('./pages/PatientCreate'))
const PatientDetail = lazy(() => import('./pages/PatientDetail'))
const PatientEdit = lazy(() => import('./pages/PatientEdit'))
const ReportDetail = lazy(() => import('./pages/ReportDetail'))
const TrendAnalysis = lazy(() => import('./pages/TrendAnalysis'))
const EditLogs = lazy(() => import('./pages/EditLogs'))
const ExpenseDetail = lazy(() => import('./pages/ExpenseDetail'))
const Login = lazy(() => import('./pages/Login'))
const Register = lazy(() => import('./pages/Register'))
const Settings = lazy(() => import('./pages/Settings'))
const AdminUsers = lazy(() => import('./pages/AdminUsers'))
const ReportCompare = lazy(() => import('./pages/ReportCompare'))
const Timeline = lazy(() => import('./pages/Timeline'))
const Medications = lazy(() => import('./pages/Medications'))
const ReportTemplates = lazy(() => import('./pages/ReportTemplates'))
const HealthAssessment = lazy(() => import('./pages/HealthAssessment'))

// Initialize auth on app load
initAuth()

function AuthGuard(props: ParentProps) {
  const navigate = useNavigate()
  createEffect(() => {
    if (authReady() && !isAuthenticated()) navigate('/login', { replace: true })
  })
  return (
    <Show
      when={authReady() && isAuthenticated()}
      fallback={
        <div class="flex flex-col items-center justify-center py-20 gap-3">
          <Spinner size="xl" variant="orbital" />
          <span class="text-sm text-content-secondary">加载中...</span>
        </div>
      }
    >
      {props.children}
    </Show>
  )
}

function GuestOnly(props: ParentProps) {
  const navigate = useNavigate()
  createEffect(() => {
    if (authReady() && isAuthenticated()) navigate('/', { replace: true })
  })
  return (
    <Show
      when={authReady() && !isAuthenticated()}
      fallback={
        <div class="flex flex-col items-center justify-center py-20 gap-3">
          <Spinner size="xl" variant="orbital" />
          <span class="text-sm text-content-secondary">加载中...</span>
        </div>
      }
    >
      {props.children}
    </Show>
  )
}

function NotFound() {
  return (
    <div class="flex flex-col items-center justify-center py-20 gap-4">
      <h1 class="text-4xl font-bold text-content">404</h1>
      <p class="text-content-secondary">页面不存在</p>
      <A href="/" class="text-accent hover:underline">返回首页</A>
    </div>
  )
}

function AuthPageShell(props: ParentProps) {
  return <>{props.children}</>
}

function App() {
  return (
    <ToastProvider>
      <Router>
        <Suspense fallback={<div class="flex flex-col items-center justify-center py-20 gap-3"><Spinner size="xl" variant="orbital" /><span class="text-sm text-content-secondary">加载中...</span></div>}>
          {/* Guest-only routes (no AppLayout) */}
          <Route path="/login" component={AuthPageShell}>
            <Route path="/" component={() => <GuestOnly><Login /></GuestOnly>} />
          </Route>
          <Route path="/register" component={AuthPageShell}>
            <Route path="/" component={() => <GuestOnly><Register /></GuestOnly>} />
          </Route>

          {/* Protected routes (with AppLayout) */}
          <Route path="/" component={(p) => <AuthGuard><AppLayout {...p} /></AuthGuard>}>
            <Route path="/" component={Dashboard} />
            <Route path="/patients/new" component={PatientCreate} />
            <Route path="/patients/:id" component={PatientDetail} />
            <Route path="/patients/:id/edit" component={PatientEdit} />
            <Route path="/patients/:id/trends" component={TrendAnalysis} />
            <Route path="/reports/:id" component={ReportDetail} />
            <Route path="/expenses/:id" component={ExpenseDetail} />
            <Route path="/edit-logs" component={EditLogs} />
            <Route path="/settings" component={Settings} />
            <Route path="/admin/users" component={AdminUsers} />
            <Route path="/patients/:id/compare" component={ReportCompare} />
            <Route path="/patients/:id/timeline" component={Timeline} />
            <Route path="/patients/:id/medications" component={Medications} />
            <Route path="/patients/:id/templates" component={ReportTemplates} />
            <Route path="/patients/:id/health-assessment" component={HealthAssessment} />
            <Route path="*" component={NotFound} />
          </Route>
        </Suspense>
      </Router>
      <ReloadPrompt />
      <PWAInstallPrompt />
    </ToastProvider>
  )
}

export default App
