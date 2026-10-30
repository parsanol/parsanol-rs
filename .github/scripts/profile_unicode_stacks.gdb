set pagination off
set confirm off
set auto-load python-scripts off
handle SIGSTOP stop print nopass
handle SIGINT stop print nopass
run
python
import gdb, os, signal, threading
pid = gdb.selected_inferior().pid
assert pid > 0
for index in range(30):
    timer = threading.Timer(0.03, lambda: os.kill(pid, signal.SIGINT))
    timer.start()
    gdb.execute('continue')
    timer.cancel()
    print('NATIVE STACK SAMPLE', index)
    gdb.execute('bt 20')
end
continue
