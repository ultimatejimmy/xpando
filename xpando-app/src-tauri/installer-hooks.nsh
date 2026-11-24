; Runs inside the NSIS installer and uninstaller. The engine runs as its own process
; (xpando-engine.exe) and would keep its file locked, so stop it before files are replaced.
!macro NSIS_HOOK_PREINSTALL
  nsExec::Exec 'taskkill /F /IM xpando-engine.exe'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec 'taskkill /F /IM xpando-engine.exe'
!macroend
