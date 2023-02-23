search_dir="../../test/message-generator/test/"
message_generator_dir="./utils/message-generator/"

cd $message_generator_dir

for entry in `ls $search_dir`; do
    echo $entry
<<<<<<< HEAD
    cargo run -- ../$search_dir$entry || { echo 'mg test failed' ; exit 1; }
=======
    cargo run -- $search_dir$entry
>>>>>>> 536ed3e (Add documentation and last fixes)
done
