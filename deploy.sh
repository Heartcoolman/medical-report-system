#!/usr/bin/env bash
set -e

cd "$(dirname "$0")"

echo "========================================="
echo "  医疗报告管理系统 - 一键部署"
echo "========================================="

# Check prerequisites
check_cmd() {
    if ! command -v "$1" &> /dev/null; then
        echo "[错误] 未安装 $1，请先安装: $2"
        exit 1
    fi
}

check_cmd node "https://nodejs.org/"
check_cmd npm "https://nodejs.org/"
check_cmd cargo "https://rustup.rs/"

echo "[信息] 环境检查通过 (Node $(node -v), $(cargo -V))"

# Generate .env if not exists
if [ ! -f .env ]; then
    echo "[信息] 生成 .env 配置文件..."

    JWT_SECRET=$(openssl rand -base64 48 2>/dev/null || head -c 48 /dev/urandom | base64)
    DB_ENCRYPTION_KEY=$(openssl rand -hex 32 2>/dev/null || head -c 32 /dev/urandom | xxd -p -c 64)

    cat > .env << EOF
# === 必填：安全配置 ===
JWT_SECRET=${JWT_SECRET}
DB_ENCRYPTION_KEY=${DB_ENCRYPTION_KEY}

# === 可选：API 密钥（用户也可在设置页面自行配置）===
LLM_API_KEY=
INTERPRET_API_KEY=
SILICONFLOW_API_KEY=

# === 可选：生产环境配置 ===
# ALLOWED_ORIGINS=https://your-domain.com
# FORCE_HTTPS=true
# PORT=3001
EOF

    echo "[信息] 已生成 .env，可按需编辑 API 密钥"
else
    echo "[信息] 检测到已有 .env 文件"
fi

mkdir -p data uploads static

# Build frontend
echo "[信息] 构建前端..."
cd frontend && npm install --silent && npm run build && cd ..
cp -r frontend/dist/* static/

# Build backend
echo "[信息] 构建后端（首次编译较慢）..."
cd backend && cargo build --release 2>&1 | tail -3 && cd ..

# Load env and start
echo ""
echo "========================================="
echo "  构建完成，启动服务..."
echo "========================================="

set -a
source .env
set +a
export PORT=${PORT:-3001}

echo "  访问地址: http://localhost:${PORT}"
echo "  首次使用请注册管理员账号"
echo "  Ctrl+C 停止服务"
echo "========================================="

exec ./backend/target/release/backend
