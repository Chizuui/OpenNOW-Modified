{
  "targets": [
    {
      "target_name": "opennow_input",
      "sources": [
        "src-native/opennow_input.cpp",
        "src-native/win_input_hook.cpp",
        "src-native/udp_client.cpp"
      ],
      "include_dirs": ["<!@(node -p \"require('node-addon-api').include\")"],
      "dependencies": ["<!(node -p \"require('node-addon-api').gyp\")"],
      "defines": ["NAPI_DISABLE_CPP_EXCEPTIONS"],
      "libraries": [
        "-lUser32.lib",
        "-lWs2_32.lib"
      ]
    }
  ]
}
