#include <sv2.h>
#include <iostream>
#include <unistd.h>
#include <stdio.h>
#include <sys/socket.h>
#include <stdlib.h>
#include <netinet/in.h>
#include <string.h>

using namespace std;
#define PORT 8080

int main()
{
    int server_fd, new_socket, valread;
    struct sockaddr_in address;
    int opt = 1;
    int addrlen = sizeof(address);
       
    // Creating socket file descriptor
    if ((server_fd = socket(AF_INET, SOCK_STREAM, 0)) == 0)
    {
        perror("socket failed");
        exit(EXIT_FAILURE);
    }
       
    // Forcefully attaching socket to the port 8080
    if (setsockopt(server_fd, SOL_SOCKET, SO_REUSEADDR | SO_REUSEPORT,
                                                  &opt, sizeof(opt)))
    {
        perror("setsockopt");
        exit(EXIT_FAILURE);
    }
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = INADDR_ANY;
    address.sin_port = htons( PORT );
       
    // Forcefully attaching socket to the port 8080
    if (bind(server_fd, (struct sockaddr *)&address, 
                                 sizeof(address))<0)
    {
        perror("bind failed");
        exit(EXIT_FAILURE);
    }
    if (listen(server_fd, 3) < 0)
    {
        perror("listen");
        exit(EXIT_FAILURE);
    }
    if ((new_socket = accept(server_fd, (struct sockaddr *)&address, 
                       (socklen_t*)&addrlen))<0)
    {
        perror("accept");
        exit(EXIT_FAILURE);
    }

    /// Istanciate Sv2 decoder
    DecoderWrapper* decoder = new_decoder();

    int byte_read = 0;
    while (true) {
            CVec buffer = get_writable(decoder);

            cout << "Buffer len: ";
            cout << buffer.len;
            cout << "\n";

            while (byte_read < buffer.len) {
                byte_read += read(new_socket, buffer.data, (buffer.len - byte_read));

                cout << "Bytes read: ";
                cout << byte_read;
                cout << "\n";

            }
            byte_read = 0;
            CResult<CSv2Message, VoidError> frame = next_frame(decoder);
            //cout << frame.ok._0.setup_connection._0.protocol;
            //cout << "\n";
    } 

    return 0;
}
