; Explorer right-click entry "Lock with PrivaVault" for files and folders (current user, so it
; goes with installMode currentUser). Explorer starts one process per selected item; the app's
; single-instance plugin forwards them all into the same window and selection.

!macro PRIVAVAULT_MENU KEY
  ${If} $LANGUAGE == 1055
    WriteRegStr SHCTX "Software\Classes\${KEY}\shell\PrivaVault" "" "PrivaVault ile kilitle"
  ${Else}
    WriteRegStr SHCTX "Software\Classes\${KEY}\shell\PrivaVault" "" "Lock with PrivaVault"
  ${EndIf}
  WriteRegStr SHCTX "Software\Classes\${KEY}\shell\PrivaVault" "Icon" "$INSTDIR\${MAINBINARYNAME}.exe"
  WriteRegStr SHCTX "Software\Classes\${KEY}\shell\PrivaVault\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" "%1"'
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro PRIVAVAULT_MENU "*"
  !insertmacro PRIVAVAULT_MENU "Directory"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegKey SHCTX "Software\Classes\*\shell\PrivaVault"
  DeleteRegKey SHCTX "Software\Classes\Directory\shell\PrivaVault"
!macroend
