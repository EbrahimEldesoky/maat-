#!/bin/bash
# ==========================================
#  maat (ماعت) - Master Startup Script
# ==========================================
set -e

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$DIR"

echo "=========================================="
echo "  ماعت (maat) - WhatsApp AI Assistant"
echo "=========================================="

# 1. تشغيل بيئة Evolution API و Postgres
echo "[1] Starting Evolution WhatsApp Engine..."
cd "$DIR/evolution_api"
docker compose up -d

for i in {1..20}; do
    if curl -s http://localhost:8085 > /dev/null 2>&1; then
        break
    fi
    sleep 1
done

cd "$DIR"

# 2. ضبط إعدادات الاتصال الدائم (Always Online) والـ Webhook
echo "[2] Ensuring 24/7 WhatsApp Connection & Webhook..."
curl -s -X POST http://localhost:8085/webhook/set/maat \
  -H "apikey: maat_evolution_secret_key_2026" \
  -H "Content-Type: application/json" \
  -d '{
    "webhook": {
      "enabled": true,
      "url": "http://host.docker.internal:8080/evolution/webhook",
      "byEvents": false,
      "base64": false,
      "events": ["MESSAGES_UPSERT"]
    }
  }' > /dev/null 2>&1 || true

curl -s -X POST http://localhost:8085/settings/set/maat \
  -H "apikey: maat_evolution_secret_key_2026" \
  -H "Content-Type: application/json" \
  -d '{
    "rejectCall": false,
    "groupsIgnore": false,
    "alwaysOnline": true,
    "readMessages": false,
    "readStatus": false,
    "syncFullHistory": false
  }' > /dev/null 2>&1 || true

echo "    Connection: Always Online Active"
echo "    Webhook: Connected to maat backend"

# 3. إيقاف أي نسخة سابقة من السيرفر
pkill -f "target/debug/maat" 2>/dev/null || true
pkill -f "target/release/maat" 2>/dev/null || true
sleep 1

# 4. تشغيل محرك ماعت للذكاء الاصطناعي
echo "[3] Starting maat AI Engine (Rust)..."
echo ""
echo "=========================================="
echo "  🚀 AI ASSISTANT IS LIVE 24/7!"
echo "=========================================="
echo "  Permanent WhatsApp Link:"
echo "  👉 https://wa.me/201043045304?text=hi%20maat"
echo "=========================================="
echo "  Server Health: http://localhost:8080/health"
echo "=========================================="
echo ""

cargo run
