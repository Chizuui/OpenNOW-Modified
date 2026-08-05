# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

OpenNOW is an open-source desktop client for GeForce NOW, built with Electron, React, and TypeScript. The active implementation lives in `opennow-stable/`. The repository also contains a native Rust streaming backend in `native/` and an iOS prototype in `ios/`.

## Common Commands

All commands should be run from the repository root, which acts as a workspace. The root `package.json` scripts proxy to the `opennow-stable` application.

- **`npm run dev`**: Starts the Electron app in development mode.
- **`npm run build`**: Builds the Electron app for production.
- **`npm run dist`**: Builds and packages the app for distribution.
- **`npm run typecheck`**: Runs TypeScript type checking for the entire project.
- **`npm run test`**: Runs the test suite. To run a single test, use `npm --prefix opennow-stable test -- --test-name-pattern <pattern>`.
- **`npm run lint`**: Lints the source code.
- **`npm run locales:check`**: Checks for issues with localization files.

## Architecture

The application is a standard Electron app with three main parts:

1.  **Main Process** (`opennow-stable/src/main`): Handles all backend logic, including:
    - GFN session orchestration and network requests (`gfn/`).
    - Native process management (`nativeStreamer/`).
    - IPC handlers for communication with the renderer (`ipc/`).
    - Filesystem access and Electron API usage.

2.  **Preload Script** (`opennow-stable/src/preload`): A bridge between the main and renderer processes. It exposes a minimal, typed API to the renderer via `window.openNow`. Raw Node.js or Electron APIs should not be exposed.

3.  **Renderer Process** (`opennow-stable/src/renderer`): The React-based user interface. It is responsible for rendering the UI and handling user interactions. All communication with the main process should go through the API exposed by the preload script.

4.  **Shared Contracts** (`opennow-stable/src/shared`): This directory contains TypeScript code shared between the main, preload, and renderer processes. This includes type definitions for IPC messages and API interfaces. This ensures type safety across process boundaries.

5.  **Native Streamer** (`native/opennow-streamer`): A Rust-based component that handles the low-level details of video streaming. The main process communicates with this native component.

## Development Guidelines

- **Module Boundaries**:
    - Main-process GFN protocol details belong under `opennow-stable/src/main/gfn`.
    - Avoid duplicating constants or logic. Extract shared functionality into focused modules.
- **Process Boundaries**:
    - Keep the renderer process focused on UI. Backend logic belongs in the main process.
    - The preload script is the gatekeeper. Do not leak backend concerns into the renderer.
- **Shared Contracts**:
    - When changing shared types or interfaces, update all consumers across all processes in the same commit.
- **Localization**:
    - All localized strings are managed via Crowdin.
    - To change or add a string, edit only `locales/en.json`. Do not edit other locale files manually.

### Critical Files for Implementation
- opennow-stable/package.json
- opennow-stable/src/shared/ipc.ts
- opennow-stable/src/main/index.ts
- opennow-stable/src/preload/index.ts
- opennow-stable/src/renderer/src/App.tsx