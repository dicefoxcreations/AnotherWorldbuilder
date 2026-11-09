; AW Server — the same program with --server — gets a Start menu entry of its own.
!macro NSIS_HOOK_POSTINSTALL
  CreateShortCut "$SMPROGRAMS\AW Server.lnk" "$INSTDIR\${MAINBINARYNAME}.exe" "--server" "$INSTDIR\aw-server.ico" 0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Delete "$SMPROGRAMS\AW Server.lnk"
  ; Started with the login, it would point at a program no longer there.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "AW Server"
!macroend
