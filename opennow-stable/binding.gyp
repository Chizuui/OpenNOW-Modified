{
  "targets": [
    {
      "target_name": "opennow_input",
      "sources": [
        "src-native/opennow_input.cpp",
        "src-native/mouse_event.h"
      ],
      "include_dirs": ["<!@(node -p \"require('node-addon-api').include\")"],
      "dependencies": ["<!(node -p \"require('node-addon-api').gyp\")"],
      "defines": ["NAPI_DISABLE_CPP_EXCEPTIONS"],
      "conditions": [
        ["OS=='win'", {
          "sources": ["src-native/win_input_hook.cpp"],
          "libraries": ["user32.lib"],
          "msvs_settings": {
            "VCCLCompilerTool": {
              "ExceptionHandling": 1,
              "AdditionalOptions": ["/std:c++17"]
            }
          }
        }],
        ["OS=='mac'", {
          "sources": ["src-native/mac_input_hook.mm"],
          "libraries": [
            "-framework Cocoa",
            "-framework CoreGraphics",
            "-framework CoreFoundation"
          ],
          "xcode_settings": {
            "CLANG_CXX_LANGUAGE_STANDARD": "c++17",
            "CLANG_CXX_LIBRARY": "libc++",
            "CLANG_ENABLE_OBJC_ARC": "NO"
          }
        }],
        ["OS=='linux'", {
          "sources": ["src-native/linux_input_hook.cpp"],
          "libraries": ["-lX11"],
          "cflags_cc": ["-std=c++17"]
        }]
      ]
    }
  ]
}
