#!/bin/bash

# PID variable to keep track of the recording process
RECORDING_PID=0

# Run the detect_movement application and pipe its output
./target/debug/motion-detect | while read line
do
    if [[ "$line" == "start" && "$RECORDING_PID" -eq 0 ]]; then
        echo "Motion detected."
        # ./start_recording &
        # RECORDING_PID=$!
        RECORDING_PID=1
    elif [[ "$line" == "stop" && "$RECORDING_PID" -ne 0 ]]; then
        echo "Motion stopped."
        # kill $RECORDING_PID
        RECORDING_PID=0
    fi
done



