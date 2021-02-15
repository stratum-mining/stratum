#include <guix-example.h>
#include <cstdint>
#include <stdio.h>
#include <assert.h>

int main() {
        ffi::Point P1;
        ffi::Point P2;

        P1.x = 10;
        P1.y = 10;

        P2.x = 10;
        P2.y = 10;

        ffi::Point P3 = ffi::sum(P1, P2);

        assert(20 == P3.x);
        assert(20 == P3.y);
        printf("guix-example ok\n");
        return 0;
}
