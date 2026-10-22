The app icon master is the owner's render ../formdesign_icon_macos.png (1024 px, an 824 px tile with a
100 px margin and drop shadow). Every icon file in the repo is a resize of it: the macOS set
(icon.icns, icon.png, 128 px and up, apple-touch where transparent, the assets PNGs) uses the
master as given; sizes of 64 px and below, the Square*Logo and StoreLogo set, the .ico files and
the favicons use its tile (x and y 100 to 923) resized and clipped to the rounded tile.

gen.py, render.mjs and build.py here draw the earlier icon and are kept for history only: running
them would overwrite the owner's icon. Do not run them; resize the master instead.
