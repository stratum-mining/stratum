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

void on_success(CSv2Message message) {
  cout << "PARSED FRAME:\n";
  cout << "  Message version: ";
  cout << message.setup_connection._0.min_version;
  cout << "\n";
  switch (message.tag) {

  case CSv2Message::Tag::SetupConnection:

    switch (message.setup_connection._0.protocol) {
      case Protocol::MiningProtocol:
        cout << "MiningProtocol \n";
        drop_sv2_message(message);
        break;
      case Protocol::TemplateDistributionProtocol:

        cout << "  Protocol: TDP \n";
        cout << "  Vendor: ";
        cout << message.setup_connection._0.vendor.data;
        cout << "\n";
        cout << "  H Version: ";
        cout << message.setup_connection._0.hardware_version.data;
        cout << "\n";
        cout << "  Firmware: ";
        cout << message.setup_connection._0.firmware.data;
        cout << "\n";
        cout << "  Device ID: ";
        cout << message.setup_connection._0.device_id.data;
        cout << "\n";

        drop_sv2_message(message);
        break;
      }
  }
}

void on_error(Sv2Error error) {
  switch (error.tag) {
  case Sv2Error::Tag::MissingBytes:
    cout << "Waiting for the remaining part of the frame \n";
    break;
  default:
    cout << "An unkwon error occured \n";
    break;
  }
}

void send_setup_connection_error(int socket, EncoderWrapper *encoder) {
  const char* error = "connection can not be created";
  uint8_t* error_ = (uint8_t*) error;

  CVec error_code = cvec_from_buffer(error_, strlen(error));
  CSetupConnectionError message;
  message.flags = 0;
  message.error_code = error_code;

  CSv2Message response;
  response.tag = CSv2Message::Tag::SetupConnectionError;
  response.setup_connection_error._0 = message;

  CResult<CVec, Sv2Error> encoded = encode(&response, encoder);
  switch (encoded.tag) {

  case CResult < CVec, Sv2Error > ::Tag::Ok:
    cout << "sending connection setup error \n";
    write(socket, encoded.ok._0.data, encoded.ok._0.len);
    drop_sv2_message(response);
    flush_encoder(encoder);
    break;
  case CResult < CVec, Sv2Error > ::Tag::Err:
    cout << "Some error occurred \n";
    break;
  };

  //char *hello = "Hello";
  //write(socket, hello, strlen(hello));
}

int main() {
  int server_fd, new_socket, valread;
  struct sockaddr_in address;
  int opt = 1;
  int addrlen = sizeof(address);

  // Creating socket file descriptor
  if ((server_fd = socket(AF_INET, SOCK_STREAM, 0)) == 0) {
    perror("socket failed");
    exit(EXIT_FAILURE);
  }

  // Forcefully attaching socket to the port 8080
  if (setsockopt(server_fd, SOL_SOCKET, SO_REUSEADDR | SO_REUSEPORT, &
      opt, sizeof(opt))) {
    perror("setsockopt");
    exit(EXIT_FAILURE);
  }
  address.sin_family = AF_INET;
  address.sin_addr.s_addr = INADDR_ANY;
  address.sin_port = htons(PORT);

  // Forcefully attaching socket to the port 8080
  if (bind(server_fd, (struct sockaddr * ) & address,
      sizeof(address)) < 0) {
    perror("bind failed");
    exit(EXIT_FAILURE);
  }
  if (listen(server_fd, 3) < 0) {
    perror("listen");
    exit(EXIT_FAILURE);
  }
  if ((new_socket = accept(server_fd, (struct sockaddr * ) & address,
      (socklen_t * ) & addrlen)) < 0) {
    perror("accept");
    exit(EXIT_FAILURE);
  }

  // Istanciate Sv2 decoder
  DecoderWrapper * decoder = new_decoder();
  EncoderWrapper * encoder = new_encoder();

  int byte_read = 0;

  while (true) {
    CVec buffer = get_writable(decoder);

    while (byte_read < buffer.len) {
      byte_read += read(new_socket, buffer.data, (buffer.len - byte_read));

    }

    byte_read = 0;
    CResult < CSv2Message, Sv2Error > frame = next_frame(decoder);


    switch (frame.tag) {

    case CResult < CSv2Message, Sv2Error > ::Tag::Ok:
      on_success(frame.ok._0);
      cout << "\n";
      cout << "START PARSING NEW FRAME";
      cout << "\n";
      send_setup_connection_error(new_socket, encoder);
      break;
    case CResult < CSv2Message, Sv2Error > ::Tag::Err:
      on_error(frame.err._0);
      break;
    };
  }

  return 0;
}

