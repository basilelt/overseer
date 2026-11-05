#!/bin/sh
set -eu
: "${ANDROID_HOME:?ANDROID_HOME is not set}"
ndk=${NDK_HOME:-}
if [ -z "$ndk" ]; then
  ndk=$(ls -d "$ANDROID_HOME"/ndk/*/ 2>/dev/null | sort -V | tail -1)
  ndk=${ndk%/}
fi
if [ -z "$ndk" ] || [ ! -d "$ndk" ]; then
  echo "::error title=android::no NDK under $ANDROID_HOME/ndk"
  exit 1
fi
bt=$(ls -d "$ANDROID_HOME"/build-tools/*/ 2>/dev/null | sort -V | tail -1)
if [ -n "${GITHUB_ENV:-}" ]; then
  echo "NDK_HOME=$ndk" >>"$GITHUB_ENV"
  [ -n "$bt" ] && echo "${bt%/}" >>"$GITHUB_PATH"
fi
echo "ANDROID_HOME=$ANDROID_HOME NDK_HOME=$ndk build-tools=${bt%/}"
if [ $# -gt 0 ]; then
  rustup target add "$@"
fi
