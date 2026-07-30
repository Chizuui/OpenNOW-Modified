import { createRequire } from 'module';
import dgram from 'dgram';

const require = createRequire(import.meta.url);
const opennowInput = require('./build/Release/opennow_input.node');

const PORT = 9000;
const HOST = '127.0.0.1';

// Setup local UDP socket receiver to print packets
const server = dgram.createSocket('udp4');

server.on('message', (msg, rinfo) => {
  // Parse GfnInputPayload
  // struct GfnInputPayload {
  //     uint8_t inputType; // 0x01 = Mouse, 0x02 = Keyboard
  //     int32_t deltaX;    // Untuk Mouse
  //     int32_t deltaY;    // Untuk Mouse
  //     uint32_t keyCode;  // Virtual Key Code (VK_*) untuk Keyboard
  //     uint8_t keyState;  // 1 = KeyDown, 0 = KeyUp
  // };
  const inputType = msg.readUInt8(0);
  if (inputType === 1) {
    const deltaX = msg.readInt32LE(1);
    const deltaY = msg.readInt32LE(5);
    console.log(`[UDP Mouse] DeltaX: ${deltaX}, DeltaY: ${deltaY}`);
  } else if (inputType === 2) {
    const keyCode = msg.readUInt32LE(9);
    const keyState = msg.readUInt8(13);
    console.log(`[UDP Keyboard] KeyCode: ${keyCode} (${vkCodeToString(keyCode)}), State: ${keyState ? 'DOWN' : 'UP'}`);
  }
});

server.bind(PORT, HOST, () => {
  console.log(`UDP Receiver listening on ${HOST}:${PORT}`);
  
  console.log('Starting Input Capture for 10 seconds...');
  console.log('Move your mouse and press keys (especially ESC).');
  console.log('ESC key will be captured and swallowed (blocked).');
  
  opennowInput.startCapture(HOST, PORT);
  
  setTimeout(() => {
    console.log('Stopping Input Capture...');
    opennowInput.stopCapture();
    server.close();
    console.log('Test completed successfully!');
    process.exit(0);
  }, 10000);
});

function vkCodeToString(vk) {
  switch(vk) {
    case 0x1B: return 'ESC';
    case 0x0D: return 'ENTER';
    case 0x20: return 'SPACE';
    default: return `VK_0x${vk.toString(16).toUpperCase()}`;
  }
}
