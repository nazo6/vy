#!/usr/bin/env bash
set -euo pipefail

# Scripts directory
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="Vy.app"
BUILD_MODE="${1:-release}"
TARGET_DIR="${ROOT_DIR}/target/${BUILD_MODE}"
BUNDLE_DIR="${TARGET_DIR}/${APP_NAME}"
CONTENTS_DIR="${BUNDLE_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"

echo "Building vy-app and vy-server (${BUILD_MODE})..."
if [ "${BUILD_MODE}" = "release" ]; then
    cargo build --release -p vy-app -p vy-server
else
    cargo build -p vy-app -p vy-server
fi

# Ensure icons are generated
if [ ! -f "${ROOT_DIR}/assets/icon.icns" ]; then
    echo "Generating icons..."
    python3 "${ROOT_DIR}/scripts/generate-icons.py"
fi

echo "Creating ${APP_NAME} bundle at ${BUNDLE_DIR}..."
rm -rf "${BUNDLE_DIR}"
mkdir -p "${MACOS_DIR}" "${RESOURCES_DIR}"

# Copy binaries
cp "${TARGET_DIR}/vy-app" "${MACOS_DIR}/vy-app"
cp "${TARGET_DIR}/vy-server" "${MACOS_DIR}/vy-server"

# Copy Info.plist and icon
cp "${ROOT_DIR}/extra/macos/Info.plist" "${CONTENTS_DIR}/Info.plist"
cp "${ROOT_DIR}/assets/icon.icns" "${RESOURCES_DIR}/icon.icns"

echo "Successfully built ${BUNDLE_DIR}"

