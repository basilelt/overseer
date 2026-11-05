on open location theURL
	tell application "Terminal"
		activate
		do script "overseer open " & quoted form of theURL
	end tell
end open location
