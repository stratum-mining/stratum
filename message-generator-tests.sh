search_dir="../../test/message-generator/test/"
message_generator_dir="./utils/message-generator/"

cd $message_generator_dir

for entry in `ls $search_dir`; do
    echo $entry
    cargo run -- ../$search_dir$entry
done
