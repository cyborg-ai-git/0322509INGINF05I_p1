-- Riceve quattro comandi gia quotati da Bash e apre una finestra per ciascuno.
-- Non esegue stringhe come codice AppleScript e non riutilizza terminali esistenti.
on run commands
    if (count of commands) is not 4 then error "Servono esattamente quattro comandi ATM."
    tell application "Terminal"
        launch
        repeat with node from 1 to 4
            set terminalTab to do script (item node of commands)
            set custom title of terminalTab to "ATM" & node
        end repeat
        activate
    end tell
end run
