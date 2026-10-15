#!/bin/bash
# 走 fvm 项目配置（.fvmrc）解析 SDK：勿写死版本路径——fvm 4.x 换了
# 版本目录布局（~/fvm/versions），旧 ~/.fvm/versions/3.47.4 残目录已缺
# bin/dart，硬编码路径会让 native assets hook 报 Invalid SDK hash。
exec fvm flutter "$@"
