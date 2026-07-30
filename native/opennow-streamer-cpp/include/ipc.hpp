#pragma once
// =============================================================================
// ipc.hpp — stdin/stdout JSON-lines IPC loop + async event writer
// =============================================================================

#include "protocol.hpp"
#include <atomic>
#include <functional>
#include <mutex>
#include <queue>
#include <condition_variable>
#include <thread>

namespace ipc {

// ---------------------------------------------------------------------------
// AsyncEventWriter
//
// Background thread that drains an event queue and writes JSON lines to stdout.
// GStreamer/decoder callbacks can safely push events from any thread.
// ---------------------------------------------------------------------------
class AsyncEventWriter {
public:
    AsyncEventWriter();
    ~AsyncEventWriter();

    // Thread-safe: enqueue an event to be written to stdout
    void push(nlohmann::json event);

    // Request shutdown and wait for the writer thread to drain + exit
    void shutdown();

private:
    void run();

    std::queue<nlohmann::json> queue_;
    std::mutex mutex_;
    std::condition_variable cv_;
    std::atomic<bool> running_{true};
    std::thread thread_;
};

// ---------------------------------------------------------------------------
// IpcDispatcher
//
// Synchronous command dispatcher called from the main loop.
// Register handlers before calling run().
// ---------------------------------------------------------------------------
using CommandHandler = std::function<bool(const protocol::CommandEnvelope&)>;
//                                    ^^^^ return false to stop the loop

class IpcDispatcher {
public:
    explicit IpcDispatcher(AsyncEventWriter& writer);

    // Register a handler for a command type string
    void on(const std::string& type, CommandHandler handler);

    // Block and process stdin until EOF or a handler returns false
    void run();

    // Convenience: write a response JSON immediately (thread-safe for single thread)
    void writeResponse(const nlohmann::json& response);

    // Convenience: schedule an event via the async writer
    void sendEvent(nlohmann::json event);

private:
    AsyncEventWriter& writer_;
    std::unordered_map<std::string, CommandHandler> handlers_;

    static void writeJsonLine(const nlohmann::json& j);
};

} // namespace ipc
