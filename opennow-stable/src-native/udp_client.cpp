#include "udp_client.h"
#include <iostream>

#pragma comment(lib, "Ws2_32.lib")

UdpClient::UdpClient(const std::string& ip, int port) : sock_(INVALID_SOCKET), initialized_(false) {
    WSADATA wsaData;
    int result = WSAStartup(MAKEWORD(2, 2), &wsaData);
    if (result != 0) {
        return;
    }

    sock_ = socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP);
    if (sock_ == INVALID_SOCKET) {
        WSACleanup();
        return;
    }

    // Set non-blocking mode (FIONBIO)
    u_long mode = 1;
    ioctlsocket(sock_, FIONBIO, &mode);

    server_addr_.sin_family = AF_INET;
    server_addr_.sin_port = htons(port);
    inet_pton(AF_INET, ip.c_str(), &server_addr_.sin_addr);

    initialized_ = true;
}

UdpClient::~UdpClient() {
    if (sock_ != INVALID_SOCKET) {
        closesocket(sock_);
    }
    if (initialized_) {
        WSACleanup();
    }
}

void UdpClient::SendPayload(const uint8_t* data, size_t size) {
    if (!initialized_ || sock_ == INVALID_SOCKET) {
        return;
    }
    sendto(sock_, reinterpret_cast<const char*>(data), static_cast<int>(size), 0,
           reinterpret_cast<const sockaddr*>(&server_addr_), sizeof(server_addr_));
}
