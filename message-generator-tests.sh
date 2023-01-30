search_dir="../../test/message-generator/test/"
message_generator_dir="./utils/message-generator/"

cd $message_generator_dir

command=""

for entry in `ls $search_dir`; do
    echo $entry
    #command=$command"cargo run -- ../$search_dir$entry && "
    cargo run -- ../$search_dir$entry
done
#echo $command
#$command
