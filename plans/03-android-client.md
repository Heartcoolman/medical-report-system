# 第三步：Android 客户端开发

> 目标：基于独立化后的 API，用 Kotlin + Jetpack Compose 构建原生 Android 客户端，功能对齐 iOS 端。

---

## 1. 项目初始化

### 1.1 基本信息
- **项目名**：medical-report-android
- **包名**：`com.heartcoolman.medical`
- **最低版本**：Android 8.0 (API 26)
- **目标版本**：Android 15 (API 35)
- **语言**：Kotlin 2.0+
- **构建**：Gradle (Kotlin DSL) + Version Catalog

### 1.2 依赖清单

```toml
# gradle/libs.versions.toml
[versions]
compose-bom = "2026.01.00"
kotlin = "2.1.0"
retrofit = "2.11.0"
okhttp = "4.12.0"
moshi = "1.15.1"
hilt = "2.51"
room = "2.7.0"
coil = "2.7.0"
vico = "2.1.0"
navigation = "2.8.0"
datastore = "1.1.1"
camerax = "1.4.0"

[libraries]
# Compose
compose-bom = { module = "androidx.compose:compose-bom", version.ref = "compose-bom" }
compose-material3 = { module = "androidx.compose.material3:material3" }
compose-ui = { module = "androidx.compose.ui:ui" }
compose-navigation = { module = "androidx.navigation:navigation-compose", version.ref = "navigation" }

# Network
retrofit-core = { module = "com.squareup.retrofit2:retrofit", version.ref = "retrofit" }
retrofit-moshi = { module = "com.squareup.retrofit2:converter-moshi", version.ref = "retrofit" }
okhttp-core = { module = "com.squareup.okhttp3:okhttp", version.ref = "okhttp" }
okhttp-logging = { module = "com.squareup.okhttp3:logging-interceptor", version.ref = "okhttp" }
okhttp-sse = { module = "com.squareup.okhttp3:okhttp-sse", version.ref = "okhttp" }
moshi-kotlin = { module = "com.squareup.moshi:moshi-kotlin", version.ref = "moshi" }

# DI
hilt-android = { module = "com.google.dagger:hilt-android", version.ref = "hilt" }
hilt-compiler = { module = "com.google.dagger:hilt-android-compiler", version.ref = "hilt" }
hilt-navigation = { module = "androidx.hilt:hilt-navigation-compose", version = "1.2.0" }

# Local
room-runtime = { module = "androidx.room:room-runtime", version.ref = "room" }
room-ktx = { module = "androidx.room:room-ktx", version.ref = "room" }
room-compiler = { module = "androidx.room:room-compiler", version.ref = "room" }
datastore = { module = "androidx.datastore:datastore-preferences", version.ref = "datastore" }

# Image & Chart
coil-compose = { module = "io.coil-kt:coil-compose", version.ref = "coil" }
vico-compose = { module = "com.patrykandpatrick.vico:compose-m3", version.ref = "vico" }

# Camera
camerax-core = { module = "androidx.camera:camera-core", version.ref = "camerax" }
camerax-camera2 = { module = "androidx.camera:camera-camera2", version.ref = "camerax" }
camerax-lifecycle = { module = "androidx.camera:camera-lifecycle", version.ref = "camerax" }
camerax-view = { module = "androidx.camera:camera-view", version.ref = "camerax" }
```

---

## 2. 项目架构

```
app/src/main/java/com/heartcoolman/medical/
├── MedicalApp.kt                    # Application + Hilt 入口
├── MainActivity.kt                  # 单 Activity
│
├── data/
│   ├── api/
│   │   ├── ApiService.kt           # Retrofit 接口定义
│   │   ├── AuthInterceptor.kt      # JWT 自动附加
│   │   ├── TokenRefreshAuth.kt     # 401 自动刷新
│   │   └── model/                  # DTO（请求/响应模型）
│   │       ├── ApiResponse.kt
│   │       ├── AuthDto.kt
│   │       ├── PatientDto.kt
│   │       ├── ReportDto.kt
│   │       ├── TemperatureDto.kt
│   │       ├── ExpenseDto.kt
│   │       └── MedicationDto.kt
│   ├── local/
│   │   ├── AppDatabase.kt          # Room 数据库
│   │   ├── dao/                    # DAO 接口
│   │   └── entity/                 # Room 实体
│   └── repo/
│       ├── AuthRepository.kt
│       ├── PatientRepository.kt
│       ├── ReportRepository.kt
│       ├── TemperatureRepository.kt
│       ├── ExpenseRepository.kt
│       └── MedicationRepository.kt
│
├── di/
│   ├── AppModule.kt                # 全局依赖
│   ├── NetworkModule.kt            # Retrofit/OkHttp
│   └── DatabaseModule.kt           # Room
│
├── ui/
│   ├── theme/
│   │   ├── Theme.kt               # Material 3 主题
│   │   ├── Color.kt
│   │   └── Type.kt
│   ├── navigation/
│   │   └── NavGraph.kt            # 导航图
│   ├── components/                 # 通用组件
│   │   ├── LoadingIndicator.kt
│   │   ├── ErrorView.kt
│   │   ├── StatusBadge.kt         # 正常/偏高/偏低/危急
│   │   ├── SearchBar.kt
│   │   ├── EmptyState.kt
│   │   └── PullRefresh.kt
│   ├── auth/
│   │   ├── LoginScreen.kt
│   │   ├── RegisterScreen.kt
│   │   └── AuthViewModel.kt
│   ├── patient/
│   │   ├── PatientListScreen.kt
│   │   ├── PatientDetailScreen.kt
│   │   ├── PatientFormScreen.kt
│   │   └── PatientViewModel.kt
│   ├── report/
│   │   ├── ReportListScreen.kt
│   │   ├── ReportDetailScreen.kt
│   │   ├── OcrUploadScreen.kt
│   │   └── ReportViewModel.kt
│   ├── interpret/
│   │   ├── InterpretScreen.kt     # AI 解读（流式显示）
│   │   └── InterpretViewModel.kt
│   ├── temperature/
│   │   ├── TemperatureScreen.kt
│   │   ├── TemperatureChart.kt    # Vico 图表
│   │   ├── TimerWidget.kt         # 5 分钟计时器
│   │   └── TemperatureViewModel.kt
│   ├── expense/
│   │   ├── ExpenseListScreen.kt
│   │   ├── ExpenseUploadScreen.kt
│   │   └── ExpenseViewModel.kt
│   ├── medication/
│   │   ├── MedicationListScreen.kt
│   │   ├── MedicationFormScreen.kt
│   │   └── MedicationViewModel.kt
│   ├── timeline/
│   │   ├── TimelineScreen.kt
│   │   └── TimelineViewModel.kt
│   └── settings/
│       ├── SettingsScreen.kt
│       └── SettingsViewModel.kt
│
└── util/
    ├── DateUtil.kt
    ├── PinyinUtil.kt               # 拼音搜索支持
    ├── FileUtil.kt                 # 图片压缩
    └── Extensions.kt
```

---

## 3. 核心模块设计

### 3.1 网络层

```kotlin
// data/api/ApiService.kt
interface ApiService {
    // Auth
    @POST("auth/login")
    suspend fun login(@Body req: LoginRequest): ApiResponse<TokenPair>

    @POST("auth/refresh")
    suspend fun refreshToken(@Body req: RefreshRequest): ApiResponse<TokenPair>

    @POST("auth/register")
    suspend fun register(@Body req: RegisterRequest): ApiResponse<TokenPair>

    @GET("auth/devices")
    suspend fun getDevices(): ApiResponse<List<DeviceInfo>>

    // Patients
    @GET("patients")
    suspend fun getPatients(
        @Query("page") page: Int = 1,
        @Query("page_size") pageSize: Int = 20,
        @Query("search") search: String? = null,
    ): ApiResponse<PaginatedData<PatientDto>>

    @POST("patients")
    suspend fun createPatient(@Body req: PatientRequest): ApiResponse<PatientDto>

    @PUT("patients/{id}")
    suspend fun updatePatient(@Path("id") id: String, @Body req: PatientRequest): ApiResponse<PatientDto>

    @DELETE("patients/{id}")
    suspend fun deletePatient(@Path("id") id: String): ApiResponse<Unit>

    // Reports
    @GET("patients/{id}/reports")
    suspend fun getReports(@Path("id") patientId: String): ApiResponse<List<ReportDto>>

    @GET("reports/{id}/interpret")
    suspend fun interpretReport(@Path("id") reportId: String): ApiResponse<InterpretDto>

    // OCR
    @Multipart
    @POST("ocr/parse")
    suspend fun ocrParse(@Part image: MultipartBody.Part): ApiResponse<OcrResultDto>

    // Temperature
    @GET("patients/{id}/temperatures")
    suspend fun getTemperatures(
        @Path("id") patientId: String,
        @Query("start") start: String? = null,
        @Query("end") end: String? = null,
    ): ApiResponse<List<TemperatureDto>>

    @POST("patients/{id}/temperatures")
    suspend fun addTemperature(@Path("id") patientId: String, @Body req: TemperatureRequest): ApiResponse<TemperatureDto>

    // Expenses
    @GET("patients/{id}/expenses")
    suspend fun getExpenses(@Path("id") patientId: String): ApiResponse<List<ExpenseDto>>

    @Multipart
    @POST("patients/{id}/expenses/parse")
    suspend fun parseExpense(@Path("id") patientId: String, @Part image: MultipartBody.Part): ApiResponse<ExpenseDto>

    // Medications
    @GET("patients/{id}/medications")
    suspend fun getMedications(@Path("id") patientId: String): ApiResponse<List<MedicationDto>>

    @POST("patients/{id}/medications")
    suspend fun addMedication(@Path("id") patientId: String, @Body req: MedicationRequest): ApiResponse<MedicationDto>

    // Timeline
    @GET("patients/{id}/timeline")
    suspend fun getTimeline(@Path("id") patientId: String): ApiResponse<List<TimelineEvent>>

    // Files
    @Multipart
    @POST("files/upload")
    suspend fun uploadFile(@Part file: MultipartBody.Part): ApiResponse<FileInfo>
}
```

### 3.2 Token 自动刷新

```kotlin
// data/api/TokenRefreshAuth.kt
class TokenRefreshAuthenticator(
    private val tokenManager: TokenManager,
    private val apiServiceProvider: Provider<ApiService>,
) : Authenticator {

    private val mutex = Mutex()

    override fun authenticate(route: Route?, response: Response): Request? {
        return runBlocking {
            mutex.withLock {
                // 已经有新 token（其他请求刷新过了）
                val currentToken = tokenManager.getAccessToken()
                val requestToken = response.request.header("Authorization")?.removePrefix("Bearer ")
                if (currentToken != requestToken && currentToken != null) {
                    return@runBlocking response.request.newBuilder()
                        .header("Authorization", "Bearer $currentToken")
                        .build()
                }

                // 尝试刷新
                val refreshToken = tokenManager.getRefreshToken() ?: return@runBlocking null
                try {
                    val result = apiServiceProvider.get().refreshToken(RefreshRequest(refreshToken))
                    if (result.success && result.data != null) {
                        tokenManager.save(result.data)
                        response.request.newBuilder()
                            .header("Authorization", "Bearer ${result.data.access_token}")
                            .build()
                    } else {
                        tokenManager.clear()
                        null
                    }
                } catch (e: Exception) {
                    tokenManager.clear()
                    null
                }
            }
        }
    }
}
```

### 3.3 AI 解读流式显示

```kotlin
// ui/interpret/InterpretViewModel.kt
@HiltViewModel
class InterpretViewModel @Inject constructor(
    private val okHttpClient: OkHttpClient,
    private val tokenManager: TokenManager,
) : ViewModel() {

    var interpretText by mutableStateOf("")
        private set
    var isLoading by mutableStateOf(false)
        private set

    fun startInterpret(reportId: String) {
        isLoading = true
        interpretText = ""

        viewModelScope.launch(Dispatchers.IO) {
            val request = Request.Builder()
                .url("${BuildConfig.API_BASE}/reports/$reportId/interpret")
                .header("Authorization", "Bearer ${tokenManager.getAccessToken()}")
                .build()

            val factory = EventSources.createFactory(okHttpClient)
            factory.newEventSource(request, object : EventSourceListener() {
                override fun onEvent(es: EventSource, id: String?, type: String?, data: String) {
                    viewModelScope.launch {
                        interpretText += data
                    }
                }
                override fun onClosed(es: EventSource) {
                    isLoading = false
                }
                override fun onFailure(es: EventSource, t: Throwable?, response: Response?) {
                    isLoading = false
                }
            })
        }
    }
}

// ui/interpret/InterpretScreen.kt
@Composable
fun InterpretScreen(reportId: String, vm: InterpretViewModel = hiltViewModel()) {
    LaunchedEffect(reportId) { vm.startInterpret(reportId) }

    Column(modifier = Modifier.padding(16.dp).verticalScroll(rememberScrollState())) {
        if (vm.isLoading && vm.interpretText.isEmpty()) {
            CircularProgressIndicator()
        }
        Text(
            text = vm.interpretText,
            style = MaterialTheme.typography.bodyLarge,
        )
    }
}
```

### 3.4 体温图表

```kotlin
// ui/temperature/TemperatureChart.kt
@Composable
fun TemperatureChart(records: List<TemperatureRecord>) {
    val chartEntryModel = records
        .sortedBy { it.recordedAt }
        .mapIndexed { index, record ->
            entryOf(index.toFloat(), record.value.toFloat())
        }
        .let { entryModelOf(it) }

    Chart(
        chart = lineChart(
            lines = listOf(
                lineSpec(
                    lineColor = MaterialTheme.colorScheme.primary.toArgb(),
                    lineThicknessDp = 2f,
                )
            )
        ),
        model = chartEntryModel,
        startAxis = rememberStartAxis(),
        bottomAxis = rememberBottomAxis(
            valueFormatter = { value, _ ->
                records.getOrNull(value.toInt())
                    ?.recordedAt
                    ?.format(DateTimeFormatter.ofPattern("HH:mm"))
                    ?: ""
            }
        ),
    )
}

// 5 分钟计时器
@Composable
fun MeasureTimer() {
    var remainingSeconds by remember { mutableIntStateOf(300) }
    var running by remember { mutableStateOf(false) }

    LaunchedEffect(running) {
        if (running) {
            while (remainingSeconds > 0) {
                delay(1000)
                remainingSeconds--
            }
            // 响铃
        }
    }

    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Text(
            text = "%02d:%02d".format(remainingSeconds / 60, remainingSeconds % 60),
            style = MaterialTheme.typography.displayLarge,
            fontWeight = FontWeight.Light,
        )
        Spacer(Modifier.height(16.dp))
        Button(onClick = {
            if (!running) { remainingSeconds = 300; running = true }
            else running = false
        }) {
            Text(if (running) "停止" else "开始计时")
        }
    }
}
```

### 3.5 OCR 拍照上传

```kotlin
// ui/report/OcrUploadScreen.kt
@Composable
fun OcrUploadScreen(
    patientId: String,
    vm: ReportViewModel = hiltViewModel(),
) {
    val context = LocalContext.current

    // 相机拍照
    val cameraLauncher = rememberLauncherForActivityResult(
        TakePicturePreview()
    ) { bitmap ->
        bitmap?.let { vm.uploadOcr(patientId, it.toCompressedFile(context)) }
    }

    // 相册选择
    val pickerLauncher = rememberLauncherForActivityResult(
        PickVisualMedia()
    ) { uri ->
        uri?.let { vm.uploadOcr(patientId, it.toCompressedFile(context)) }
    }

    Column {
        // OCR 结果展示 + 编辑确认
        if (vm.ocrResult != null) {
            OcrResultEditor(vm.ocrResult!!, onConfirm = { vm.saveReport(patientId, it) })
        }

        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            OutlinedButton(onClick = { cameraLauncher.launch(null) }) {
                Icon(Icons.Default.CameraAlt, null)
                Text("拍照")
            }
            OutlinedButton(onClick = { pickerLauncher.launch(PickVisualMediaRequest(ImageOnly)) }) {
                Icon(Icons.Default.PhotoLibrary, null)
                Text("相册")
            }
        }
    }
}
```

---

## 4. UI 设计

### 4.1 主题
- Material 3 Dynamic Color（跟随系统壁纸取色）
- 支持深色/浅色模式切换
- 自定义色板作为 fallback（非 Android 12+ 设备）

### 4.2 导航结构
```
BottomNavigation:
├── 🏠 首页（患者列表）
├── 📊 报告（最近报告）
├── 🌡️ 体温（快捷记录）
└── ⚙️ 设置

患者详情页（子导航 Tab）:
├── 报告列表
├── 体温记录
├── 费用清单
├── 用药管理
└── 时间线
```

### 4.3 关键页面
| 页面 | 说明 |
|------|------|
| 登录/注册 | 简洁表单，支持记住密码 |
| 患者列表 | 搜索栏 + 列表 + FAB 添加，支持拼音搜索 |
| 患者详情 | 顶部卡片（基本信息）+ Tab 切换子模块 |
| 报告详情 | 检验项目列表，状态标色，点击可看 AI 解读 |
| AI 解读 | 流式文本，Markdown 渲染 |
| 体温页 | 图表 + 记录列表 + 计时器 |
| OCR 上传 | 拍照/相册 → 识别结果编辑 → 确认保存 |

---

## 5. 离线 & 缓存策略

```
请求 → 有网 → API 获取 → 存 Room → 显示
请求 → 无网 → 读 Room 缓存 → 显示 + 提示"离线模式"
写操作 → 无网 → 暂不支持（提示需要网络）
```

Room 缓存范围：
- 患者列表（全量）
- 最近 30 天的报告
- 最近 7 天的体温记录
- 用户信息 + 设置

---

## 6. 开发阶段

### Phase 1 — 基础框架 + 核心功能（5天）

| 天 | 任务 |
|----|------|
| D1 | 项目初始化、依赖配置、Hilt/Retrofit/Room 搭建、主题 |
| D2 | 登录/注册 + Token 管理 + 自动刷新 |
| D3 | 患者列表/搜索/CRUD + 导航框架 |
| D4 | 报告列表/详情 + 检验项目展示 |
| D5 | OCR 拍照上传 + 结果编辑确认 |

### Phase 2 — AI + 体温（3天）

| 天 | 任务 |
|----|------|
| D6 | AI 解读流式显示 + Markdown 渲染 |
| D7 | 体温记录 + Vico 图表（日/周视图） |
| D8 | 5 分钟计时器 + 多部位支持 |

### Phase 3 — 完善（3天）

| 天 | 任务 |
|----|------|
| D9 | 费用清单（图片识别 + 列表） |
| D10 | 用药管理 + 健康时间线 |
| D11 | 设置页、离线缓存、数据导出、打磨细节 |

### Phase 4 — 发布准备（2天）

| 天 | 任务 |
|----|------|
| D12 | 全流程测试 + Bug 修复 |
| D13 | 签名、ProGuard、应用图标、截图、README |

**总预估：约 13 天**

---

## 7. 仓库结构

```
medical-report-android/
├── app/
│   ├── src/main/
│   │   ├── java/com/heartcoolman/medical/
│   │   ├── res/
│   │   └── AndroidManifest.xml
│   └── build.gradle.kts
├── gradle/
│   └── libs.versions.toml
├── build.gradle.kts
├── settings.gradle.kts
├── .gitignore
└── README.md
```

---

## 任务清单

| # | 任务 | 优先级 | 预估 |
|---|------|--------|------|
| 1 | 项目初始化 + 依赖 + DI | P0 | 4h |
| 2 | 主题 + 导航框架 | P0 | 3h |
| 3 | 网络层 (Retrofit + Auth) | P0 | 4h |
| 4 | Token 自动刷新 | P0 | 3h |
| 5 | 登录/注册页 | P0 | 3h |
| 6 | 患者列表/搜索/CRUD | P0 | 6h |
| 7 | 报告列表/详情 | P0 | 4h |
| 8 | OCR 上传（相机+相册） | P0 | 5h |
| 9 | AI 解读（SSE 流式） | P1 | 4h |
| 10 | 体温记录 + 图表 | P1 | 6h |
| 11 | 计时器组件 | P1 | 2h |
| 12 | 费用清单 | P1 | 4h |
| 13 | 用药管理 | P1 | 4h |
| 14 | 健康时间线 | P2 | 3h |
| 15 | 设置页 | P2 | 2h |
| 16 | Room 缓存 + 离线 | P2 | 4h |
| 17 | 数据导出 | P2 | 3h |
| 18 | 测试 + 发布准备 | P0 | 6h |

**总预估：约 70 小时（1 人 ~13 天）**

---

## 完成标准

- [ ] 登录/注册 + Token 自动刷新
- [ ] 患者 CRUD + 拼音搜索
- [ ] 报告查看 + OCR 上传识别
- [ ] AI 智能解读（流式）
- [ ] 体温记录 + 图表 + 计时器
- [ ] 费用清单 + 用药管理
- [ ] 健康时间线
- [ ] 离线缓存基本可用
- [ ] Material 3 主题，深色/浅色适配
- [ ] 签名打包，可分发 APK
