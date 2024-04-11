#!/bin/bash

screen -ls | grep Detached | cut -d. -f1 | awk '{print $1}' | xargs -r kill

# Define the session name
SESSION="sidepoolservices"

# Function to check if the screen session exists
session_exists() {
    return $(screen -list | grep -q "$1")
}

# Create a new screen session if it doesn't exist
if session_exists "$SESSION"; then
    echo "Session $SESSION already exists. Attaching to it..."
    screen -r $SESSION
    exit 0
else
    screen -dmS $SESSION
fi

# Define your projects and commands
declare -a services=("pool" "translator" "jd-server" "jd-client")
declare -a commands=("cd $(pwd) && ./release/pool_sv2 -c config/pool.toml" 
                     "cd $(pwd) && ./release/translator_sv2 -c config/tproxy.yml" 
                     "cd $(pwd) && ./release/jd_server -c config/jds.toml" 
                     "cd $(pwd) && ./release/jd_client -c config/jdc.toml")

# Create screen windows for each service and execute commands
for i in "${!services[@]}"; do
    # Start each service in a new window with a title
    screen -S "$SESSION" -X screen -t "${services[$i]}" "${i}"
    sleep 0.5  # Wait a bit to make sure the window is ready    
    screen -S "$SESSION" -p "${i}" -X stuff "${commands[$i]}" 
done

# Setup the hardstatus line
screen -S $SESSION -X hardstatus alwayslastline "%c %w"

# Wait a bit before setting up the layout (important to allow screen commands to register)
sleep 1

# # Setup the split screen layout
# screen -S $SESSION -X select 0   # Select the first window
# screen -S $SESSION -X split -v   # Vertically split the window
# screen -S $SESSION -X focus      # Move focus to the new split section
# screen -S $SESSION -X screen -t ${services[1]} 1
# screen -S $SESSION -p 1 -X stuff "${commands[1]}\n"
# screen -S $SESSION -X split -v   # Vertically split the window
# screen -S $SESSION -X focus
# screen -S $SESSION -X screen -t ${services[2]} 2
# screen -S $SESSION -p 2 -X stuff "${commands[2]}\n"
# screen -S $SESSION -X select 0
# screen -S $SESSION -X focus      # Move focus back to the first split
# screen -S $SESSION -X resize 40  # Resize the first split to use 40% of the screen
# screen -S $SESSION -X select 2   # Select the third window
# screen -S $SESSION -X focus
# screen -S $SESSION -X split -v
# screen -S $SESSION -X focus
# screen -S $SESSION -X screen -t ${services[3]} 3
# screen -S $SESSION -p 3 -X stuff "${commands[3]}\n"

# Attach to the screen session
screen -r $SESSION
