
## Introduction
Basic tools used in RustPilot, offer the functionalities include:

- hrt(high resolution clock), used to schedule pthread or workqueue
- channel, used for intern process communication
    - provide basic rx/tx channel with no fifo(only record the latest message)
    - support msg callback
- scheduled_pthread, we can schedule a pthread periodically
- lock step support, user could provide the time update function to replace the default system clock
- module support, provide basic module register and get.
- pthread, low-level pthread wrapper, used by scheduled_pthread

