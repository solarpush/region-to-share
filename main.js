const {
  app,
  BrowserWindow,
  screen,
  ipcMain,
  globalShortcut,
} = require("electron");

let overlayWindow;
let isInteractive = false; // état courant

app.disableHardwareAcceleration();

app.whenReady().then(() => {
  const { width, height } = screen.getPrimaryDisplay().workAreaSize;

  overlayWindow = new BrowserWindow({
    width: 1920,
    height: 1080,
    x: Math.floor((width - 1920) / 2),
    y: Math.floor((height - 1080) / 2),
    transparent: true,
    frame: false,
    resizable: true,
    movable: true,
    alwaysOnTop: true,
    skipTaskbar: true,
    hasShadow: false,
    webPreferences: {
      nodeIntegration: true,
      contextIsolation: false,
    },
  });

  overlayWindow.loadFile("overlay.html");

  // état initial = click-through
  overlayWindow.setIgnoreMouseEvents(true, { forward: true });
  overlayWindow.webContents.send("mode", "clickthrough");

  // Toggle interactivité
  globalShortcut.register("Control+Shift+T", () => {
    isInteractive = !isInteractive;

    overlayWindow.setIgnoreMouseEvents(!isInteractive, { forward: true });

    overlayWindow.webContents.send(
      "mode",
      isInteractive ? "interactive" : "clickthrough"
    );
  });

  ipcMain.on("close-overlay", () => {
    overlayWindow.close();
  });
});

app.on("will-quit", () => {
  globalShortcut.unregisterAll();
});
