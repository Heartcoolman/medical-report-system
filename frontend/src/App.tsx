import { lazy, Suspense } from 'solid-js'
import { ToastProvider, Spinner } from './components'
import { Router, Route, A } from '@solidjs/router'
import AppLayout from './layouts/AppLayout'

const Dashboard = lazy(() => import('./pages/Dashboard'))
const PatientCreate = lazy(() => import('./pages/PatientCreate'))
const PatientDetail = lazy(() => import('./pages/PatientDetail'))
const PatientEdit = lazy(() => import('./pages/PatientEdit'))
const ReportDetail = lazy(() => import('./pages/ReportDetail'))
const TrendAnalysis = lazy(() => import('./pages/TrendAnalysis'))
const EditLogs = lazy(() => import('./pages/EditLogs'))
const ExpenseDetail = lazy(() => import('./pages/ExpenseDetail'))

function NotFound() {
  return (
    <div class="flex flex-col items-center justify-center py-20 gap-4">
      <h1 class="text-4xl font-bold text-content">404</h1>
      <p class="text-content-secondary">页面不存在</p>
      <A href="/" class="text-accent hover:underline">返回首页</A>
    </div>
  )
}

function App() {
  return (
    <ToastProvider>
      <Router root={AppLayout}>
        <Suspense fallback={<div class="flex flex-col items-center justify-center py-20 gap-3"><Spinner size="xl" variant="orbital" /><span class="text-sm text-content-secondary">加载中...</span></div>}>
          <Route path="/" component={Dashboard} />
          <Route path="/patients/new" component={PatientCreate} />
          <Route path="/patients/:id" component={PatientDetail} />
          <Route path="/patients/:id/edit" component={PatientEdit} />
          <Route path="/patients/:id/trends" component={TrendAnalysis} />
          <Route path="/reports/:id" component={ReportDetail} />
          <Route path="/expenses/:id" component={ExpenseDetail} />
          <Route path="/edit-logs" component={EditLogs} />
          <Route path="*" component={NotFound} />
        </Suspense>
      </Router>
    </ToastProvider>
  )
}

export default App
