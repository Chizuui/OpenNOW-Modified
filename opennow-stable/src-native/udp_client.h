#ifndef UDP_CLIENT_H
#define UDP_CLIENT_H

#include <winsock2.h>
#include <ws2tcpip.h>
#include <vector>
#include <string>
#include <cstdint>

class UdpClient {
public:
    UdpClient(const std::string& ip, int port);
    ~UdpClient();

    void SendPayload(const uint8_t* data, size_t size);

private:
    SOCKET sock_;
    sockaddr_in server_addr_;
    bool initialized_;
};

#endif // UDP_CLIENT_H
