## Usage

```bash
$ npm install # or pnpm install or yarn install
```

### Learn more on the [Solid Website](https://solidjs.com) and come chat with us on our [Discord](https://discord.com/invite/solidjs)

## Available Scripts

In the project directory, you can run:

### `npm run dev`

Runs the app in the development mode.<br>
Open [http://localhost:5173](http://localhost:5173) to view it in the browser.

### `npm run build`

Builds the app for production to the `dist` folder.<br>
It correctly bundles Solid in production mode and optimizes the build for the best performance.

The build is minified and the filenames include the hashes.<br>
Your app is ready to be deployed!

## UI Design System 规范

前端样式统一入口：`src/index.css`，新增页面/组件时优先复用语义类，不要直接拼一长串原子类。

- 主题变量：统一使用 `surface/content/accent/success/warning/error/info/border/overlay` token。
- 页面结构：优先使用 `page-shell`、`form-page`、`page-title`、`section-title`、`form-title`。
- 导航与面包屑：优先使用 `app-header-link`、`nav-link-base`、`nav-link-active`、`nav-icon-btn`、`breadcrumb`、`breadcrumb-link`。
- 表单控件：优先使用 `Input/Select/Textarea` 组件，底层复用 `form-control-*`、`form-label`、`helper-text`、`error-text`。
- 表格与空状态：优先使用 `table-header`、`table-cell`、`table-row-striped`、`empty-title`、`empty-description`。
- 语义文本：优先使用 `sub-text`、`section-subtitle`、`micro-title`、`meta-text`、`data-label`、`data-value`。

不建议：

- 在页面里直接写 `black/white` 或品牌色硬编码。
- 同一语义出现多个写法（例如标题层级、输入框样式、卡片内信息标签）。
- 用页面级临时类覆盖公共组件基础视觉，除非确有业务差异。

## Deployment

Learn more about deploying your application with the [documentations](https://vite.dev/guide/static-deploy.html)
