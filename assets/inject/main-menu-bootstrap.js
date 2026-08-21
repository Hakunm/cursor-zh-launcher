(() => {
  "use strict";

  const version = __CURSOR_ZH_VERSION__;
  const translations = new Map(__CURSOR_ZH_RULES__);
  const marker = "__cursorZhMainMenuVersion";
  const stats = globalThis.__cursorZhMainMenuStats || {
    translatedItems: 0,
    buildCalls: 0,
    applicationMenuCalls: 0,
    trayMenuCalls: 0
  };
  globalThis.__cursorZhMainMenuStats = stats;
  const electron = (() => {
    try {
      if (typeof require === "function") return require("electron");
    } catch {}
    try {
      return process.mainModule?.require?.("electron") || null;
    } catch {
      return null;
    }
  })();
  if (!electron?.Menu || !electron?.Tray) {
    return { status: "electron-unavailable", version };
  }
  if (globalThis[marker] === version) return { status: "already-installed", version };

  function translatedLabel(label) {
    if (typeof label !== "string" || !label) return label;
    const direct = translations.get(label);
    if (direct) {
      stats.translatedItems += 1;
      return direct;
    }
    const mnemonic = label.match(/^(&+)(.+)$/);
    if (!mnemonic) return label;
    const translated = translations.get(mnemonic[2]);
    if (!translated) return label;
    stats.translatedItems += 1;
    return `${mnemonic[1]}${translated}`;
  }

  function translateTemplate(items) {
    if (!Array.isArray(items)) return;
    for (const item of items) {
      if (!item || typeof item !== "object") continue;
      if (typeof item.label === "string") item.label = translatedLabel(item.label);
      if (Array.isArray(item.submenu)) translateTemplate(item.submenu);
      else if (item.submenu?.items) translateMenu(item.submenu);
    }
  }

  function translateMenu(menu) {
    if (!menu?.items) return menu;
    for (const item of menu.items) {
      if (typeof item.label === "string") item.label = translatedLabel(item.label);
      if (item.submenu) translateMenu(item.submenu);
    }
    return menu;
  }

  const Menu = electron.Menu;
  if (!Menu.__cursorZhBuildFromTemplateOriginal) {
    Menu.__cursorZhBuildFromTemplateOriginal = Menu.buildFromTemplate.bind(Menu);
    Menu.buildFromTemplate = (template) => {
      stats.buildCalls += 1;
      translateTemplate(template);
      return translateMenu(Menu.__cursorZhBuildFromTemplateOriginal(template));
    };
  }
  if (!Menu.__cursorZhSetApplicationMenuOriginal) {
    Menu.__cursorZhSetApplicationMenuOriginal = Menu.setApplicationMenu.bind(Menu);
    Menu.setApplicationMenu = (menu) => {
      stats.applicationMenuCalls += 1;
      return Menu.__cursorZhSetApplicationMenuOriginal(translateMenu(menu));
    };
  }

  const trayPrototype = electron.Tray.prototype;
  if (!trayPrototype.__cursorZhSetContextMenuOriginal) {
    trayPrototype.__cursorZhSetContextMenuOriginal = trayPrototype.setContextMenu;
    trayPrototype.setContextMenu = function setLocalizedContextMenu(menu) {
      stats.trayMenuCalls += 1;
      return trayPrototype.__cursorZhSetContextMenuOriginal.call(this, translateMenu(menu));
    };
  }

  const currentMenu = Menu.getApplicationMenu();
  if (currentMenu) Menu.setApplicationMenu(translateMenu(currentMenu));
  globalThis[marker] = version;
  return { status: "installed", version };
})()
