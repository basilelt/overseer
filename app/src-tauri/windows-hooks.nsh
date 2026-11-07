; A running daemon locks overseer.exe; Windows lets a running image be renamed, not overwritten.
!macro OVERSEER_MOVE_DAEMON ARCH
  Delete /REBOOTOK "$INSTDIR\server\${ARCH}\overseer.exe.old"
  ClearErrors
  Delete "$INSTDIR\server\${ARCH}\overseer.exe"
  ${If} ${Errors}
    Rename "$INSTDIR\server\${ARCH}\overseer.exe" "$INSTDIR\server\${ARCH}\overseer.exe.old"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro OVERSEER_MOVE_DAEMON windows-x86_64
  !insertmacro OVERSEER_MOVE_DAEMON windows-aarch64
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Delete /REBOOTOK "$INSTDIR\server\windows-x86_64\overseer.exe"
  Delete /REBOOTOK "$INSTDIR\server\windows-x86_64\overseer.exe.old"
  Delete /REBOOTOK "$INSTDIR\server\windows-aarch64\overseer.exe"
  Delete /REBOOTOK "$INSTDIR\server\windows-aarch64\overseer.exe.old"
!macroend
