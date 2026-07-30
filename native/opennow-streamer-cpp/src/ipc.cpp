#include "ipc.hpp"
#include <spdlog/spdlog.h>
#include <iostream>
#include <sstream>
#include <io.h>      // _setmode
#include <fcntl.h>   // _O_BINARY

namespace ipc {

// ---------------------------------------------------------------------------
// AsyncEventWriter
// ---------------------------------------------------------------------------
AsyncEventWriter::AsyncEventWriter()
    : thread_([this] { run(); })
{}

AsyncEventWriter::~AsyncEventWriter() {
    shutdown();
}

void AsyncEventWriter::push(nlohmann::json event) {
    {
        std::lock_guard<std::mutex> lock(mutex_);
        queue_.push(std::move(event));
    }
    cv_.notify_one();
}

void AsyncEventWriter::shutdown() {
    {
        std::lock_guard<std::mutex> lock(mutex_);
        running_.store(false);
    }
    cv_.notify_all();
    if (thread_.joinable())
        thread_.join();
}

void AsyncEventWriter::run() {
    while (true) {
        std::unique_lock<std::mutex> lock(mutex_);
        cv_.wait(lock, [this] {
            return !queue_.empty() || !running_.load();
        });

        while (!queue_.empty()) {
            auto event = std::move(queue_.front());
            queue_.pop();
            lock.unlock();

            try {
                auto line = event.dump();
                // Write to stdout with newline
                std::cout << line << '\n';
                std::cout.flush();
            } catch (const std::exception& ex) {
                spdlog::error("[IPC] Failed to write event: {}", ex.what());
            }

            lock.lock();
        }

        if (!running_.load() && queue_.empty())
            break;
    }
}

// ---------------------------------------------------------------------------
// IpcDispatcher
// ---------------------------------------------------------------------------
IpcDispatcher::IpcDispatcher(AsyncEventWriter& writer)
    : writer_(writer)
{}

void IpcDispatcher::on(const std::string& type, CommandHandler handler) {
    handlers_[type] = std::move(handler);
}

void IpcDispatcher::sendEvent(nlohmann::json event) {
    writer_.push(std::move(event));
}

void IpcDispatcher::writeResponse(const nlohmann::json& response) {
    writeJsonLine(response);
}

void IpcDispatcher::writeJsonLine(const nlohmann::json& j) {
    try {
        std::cout << j.dump() << '\n';
        std::cout.flush();
    } catch (const std::exception& ex) {
        spdlog::error("[IPC] writeJsonLine failed: {}", ex.what());
    }
}

void IpcDispatcher::run() {
    // Set stdin/stdout to binary mode to avoid CRLF translation on Windows
    _setmode(_fileno(stdin),  _O_BINARY);
    _setmode(_fileno(stdout), _O_BINARY);

    spdlog::info("[IPC] Listening on stdin...");

    std::string line;
    while (std::getline(std::cin, line)) {
        // Strip trailing CR if present (Windows CRLF)
        if (!line.empty() && line.back() == '\r')
            line.pop_back();
        if (line.empty())
            continue;

        // Parse JSON
        nlohmann::json value;
        try {
            value = nlohmann::json::parse(line);
        } catch (const nlohmann::json::exception& ex) {
            writeResponse(protocol::makeErrorResponse(
                std::nullopt, "invalid-json", ex.what()));
            continue;
        }

        // Deserialize command envelope
        protocol::CommandEnvelope cmd;
        try {
            cmd = value.get<protocol::CommandEnvelope>();
        } catch (const std::exception& ex) {
            writeResponse(protocol::makeErrorResponse(
                std::nullopt, "invalid-command", ex.what()));
            continue;
        }

        // Dispatch
        auto it = handlers_.find(cmd.type);
        if (it == handlers_.end()) {
            writeResponse(protocol::makeErrorResponse(
                cmd.id, "unknown-command",
                "Unknown command: " + cmd.type));
            continue;
        }

        bool shouldContinue = false;
        try {
            shouldContinue = it->second(cmd);
        } catch (const std::exception& ex) {
            writeResponse(protocol::makeErrorResponse(
                cmd.id, "command-error", ex.what()));
            shouldContinue = true;
        }

        if (!shouldContinue)
            break;
    }

    spdlog::info("[IPC] stdin closed, shutting down.");
}

} // namespace ipc
