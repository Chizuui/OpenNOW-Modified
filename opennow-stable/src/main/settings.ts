import { app } from "electron";
import { join } from "node:path";
import { readFileSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
import type {
  NativeStreamerBackendPreference,
  NativeVideoBackendPreference,
  ControllerThemeRgb,
  ControllerThemeStyle,
  AppAccentColor,
  Settings,
} from "@shared/gfn";
import {
  DEFAULT_SETTINGS,
  DEFAULT_SHORTCUTS,
  normalizeStreamClientModeForPlatform,
  normalizeStreamPreferences,
} from "@shared/gfn";

const LEGACY_STOP_SHORTCUTS = new Set(["META+SHIFT+Q", "CMD+SHIFT+Q"]);
const LEGACY_ANTI_AFK_SHORTCUTS = new Set(["META+SHIFT+F10", "CMD+SHIFT+F10", "CTRL+SHIFT+F10"]);

const CONTROLLER_THEME_STYLES_SET = new Set<ControllerThemeStyle>(["aurora", "nebula", "grid", "minimal", "pulse"]);
const NATIVE_VIDEO_BACKEND_PREFERENCES = new Set<NativeVideoBackendPreference>(["auto", "d3d11", "d3d12"]);
const APP_ACCENT_COLORS = new Set<AppAccentColor>(["green", "blue", "violet", "amber", "rose"]);

function clampThemeByte(value: unknown): number {
  const n = typeof value === "number" && Number.isFinite(value) ? Math.round(value) : NaN;
  if (!Number.isFinite(n)) return 0;
  return Math.max(0, Math.min(255, n));
}

function normalizeControllerThemeColor(raw: unknown, fallback: ControllerThemeRgb): ControllerThemeRgb {
  if (!raw || typeof raw !== "object") return { ...fallback };
  const o = raw as Record<string, unknown>;
  return {
    r: clampThemeByte(o.r),
    g: clampThemeByte(o.g),
    b: clampThemeByte(o.b),
  };
}

function normalizeControllerThemeStyle(raw: unknown): ControllerThemeStyle {
  return CONTROLLER_THEME_STYLES_SET.has(raw as ControllerThemeStyle) ? (raw as ControllerThemeStyle) : "aurora";
}

function normalizeNativeVideoBackendPreference(raw: unknown): NativeVideoBackendPreference {
  return NATIVE_VIDEO_BACKEND_PREFERENCES.has(raw as NativeVideoBackendPreference)
    ? (raw as NativeVideoBackendPreference)
    : "auto";
}

function normalizeAppAccentColor(raw: unknown): AppAccentColor {
  return APP_ACCENT_COLORS.has(raw as AppAccentColor) ? (raw as AppAccentColor) : "green";
}

export class SettingsManager {
  private settings: Settings;
  private readonly settingsPath: string;

  constructor() {
    this.settingsPath = join(app.getPath("userData"), "settings.json");
    this.settings = this.load();
  }

  /**
   * Load settings from disk or return defaults if file doesn't exist
   */
  private load(): Settings {
    try {
      if (!existsSync(this.settingsPath)) {
        const defaults = { ...DEFAULT_SETTINGS };
        this.enforceCompatibility(defaults);
        return defaults;
      }

      const content = readFileSync(this.settingsPath, "utf-8");
      const parsed = JSON.parse(content) as Partial<Settings>;

      // Merge with defaults to ensure all fields exist
      const merged: Settings = {
        ...DEFAULT_SETTINGS,
        ...parsed,
      };

      let migrated = this.migrateLegacyShortcutDefaults(merged);
      migrated = this.enforceCompatibility(merged) || migrated;

      const themeStyleBefore = merged.controllerThemeStyle;
      const themeColorBefore = { ...merged.controllerThemeColor };
      merged.controllerThemeStyle = normalizeControllerThemeStyle(merged.controllerThemeStyle);
      merged.controllerThemeColor = normalizeControllerThemeColor(merged.controllerThemeColor, DEFAULT_SETTINGS.controllerThemeColor);
      const accentColorBefore = merged.appAccentColor;
      merged.appAccentColor = normalizeAppAccentColor(merged.appAccentColor);
      if (
        merged.appAccentColor !== accentColorBefore ||
        merged.controllerThemeStyle !== themeStyleBefore ||
        merged.controllerThemeColor.r !== themeColorBefore.r ||
        merged.controllerThemeColor.g !== themeColorBefore.g ||
        merged.controllerThemeColor.b !== themeColorBefore.b
      ) {
        migrated = true;
      }

      // Migrate legacy boolean accelerator setting to percentage slider.
      if (typeof (parsed as { mouseAcceleration?: unknown }).mouseAcceleration === "boolean") {
        merged.mouseAcceleration = (parsed as { mouseAcceleration?: boolean }).mouseAcceleration ? 100 : 1;
        migrated = true;
      }

      merged.mouseAcceleration = Math.max(1, Math.min(150, Math.round(merged.mouseAcceleration)));
      if (migrated) {
        writeFileSync(this.settingsPath, JSON.stringify(merged, null, 2), "utf-8");
      }

      return merged;
    } catch (error) {
      console.error("Failed to load settings, using defaults:", error);
      const defaults = { ...DEFAULT_SETTINGS };
      this.enforceCompatibility(defaults);
      return defaults;
    }
  }

  private enforceCompatibility(settings: Settings): boolean {
    let migrated = false;
    const normalized = normalizeStreamPreferences(settings.codec, settings.colorQuality);
    if (normalized.migrated) {
      console.warn(
        `[Settings] Migrating unsupported stream settings codec="${settings.codec}" colorQuality="${settings.colorQuality}" to ${normalized.codec}/${normalized.colorQuality}`,
      );
      settings.codec = normalized.codec;
      settings.colorQuality = normalized.colorQuality;
      migrated = true;
    }

    const streamClientMode = normalizeStreamClientModeForPlatform(settings.streamClientMode, process.platform);
    if (settings.streamClientMode !== streamClientMode) {
      settings.streamClientMode = streamClientMode;
      migrated = true;
    }

    if (settings.nativeStreamerBackend !== "gstreamer") {
      settings.nativeStreamerBackend = "gstreamer";
      migrated = true;
    }
    const appAccentColor = normalizeAppAccentColor(settings.appAccentColor);
    if (settings.appAccentColor !== appAccentColor) {
      settings.appAccentColor = appAccentColor;
      migrated = true;
    }
    if (!settings.nativeExternalRenderer) {
      settings.nativeExternalRenderer = true;
      migrated = true;
    }
    const nativeVideoBackend = normalizeNativeVideoBackendPreference(settings.nativeVideoBackend);
    if (settings.nativeVideoBackend !== nativeVideoBackend) {
      settings.nativeVideoBackend = nativeVideoBackend;
      migrated = true;
    }

    return migrated;
  }

  private migrateLegacyShortcutDefaults(settings: Settings): boolean {
    let migrated = false;

    const normalizeShortcut = (value: string): string => value.replace(/\s+/g, "").toUpperCase();
    const stopShortcut = normalizeShortcut(settings.shortcutStopStream);
    const antiAfkShortcut = normalizeShortcut(settings.shortcutToggleAntiAfk);

    if (LEGACY_STOP_SHORTCUTS.has(stopShortcut)) {
      settings.shortcutStopStream = DEFAULT_SHORTCUTS.shortcutStopStream;
      migrated = true;
    }

    if (LEGACY_ANTI_AFK_SHORTCUTS.has(antiAfkShortcut)) {
      settings.shortcutToggleAntiAfk = DEFAULT_SHORTCUTS.shortcutToggleAntiAfk;
      migrated = true;
    }

    return migrated;
  }

  /**
   * Save current settings to disk
   */
  private save(): void {
    try {
      const dir = join(app.getPath("userData"));
      if (!existsSync(dir)) {
        mkdirSync(dir, { recursive: true });
      }

      writeFileSync(this.settingsPath, JSON.stringify(this.settings, null, 2), "utf-8");
    } catch (error) {
      console.error("Failed to save settings:", error);
    }
  }

  /**
   * Get all current settings
   */
  getAll(): Settings {
    return { ...this.settings };
  }

  /**
   * Get a specific setting value
   */
  get<K extends keyof Settings>(key: K): Settings[K] {
    return this.settings[key];
  }

  /**
   * Update a specific setting value
   */
  set<K extends keyof Settings>(key: K, value: Settings[K]): void {
    this.settings[key] = value;
    this.enforceCompatibility(this.settings);
    this.save();
  }

  /**
   * Update multiple settings at once
   */
  setMultiple(updates: Partial<Settings>): void {
    this.settings = {
      ...this.settings,
      ...updates,
    };
    this.enforceCompatibility(this.settings);
    this.save();
  }

  /**
   * Reset all settings to defaults
   */
  reset(): Settings {
    this.settings = { ...DEFAULT_SETTINGS };
    this.enforceCompatibility(this.settings);
    this.save();
    return { ...this.settings };
  }

  /**
   * Get the default settings
   */
  getDefaults(): Settings {
    const defaults = { ...DEFAULT_SETTINGS };
    this.enforceCompatibility(defaults);
    return defaults;
  }
}

// Singleton instance
let settingsManager: SettingsManager | null = null;

export function getSettingsManager(): SettingsManager {
  if (!settingsManager) {
    settingsManager = new SettingsManager();
  }
  return settingsManager;
}

export { DEFAULT_SETTINGS };
