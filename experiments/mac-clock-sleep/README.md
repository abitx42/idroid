# macOS sleep-inclusive clock probe

This is a manual macOS-only probe for the clock boundary used by the control
plane. It compares Darwin `CLOCK_MONOTONIC` with `CLOCK_UPTIME_RAW` across an
actual system sleep. It is not a product test and does not create committed
evidence.

Run it from a physical Mac when it is safe to sleep the machine:

```sh
bash experiments/mac-clock-sleep/run.sh
```

The script asks for administrator authorization, records both clocks, invokes
`pmset sleepnow`, waits for the user to wake the Mac, records both clocks again,
and prints the deltas. `CLOCK_MONOTONIC` should include the sleep interval;
`CLOCK_UPTIME_RAW` should exclude it. The control-plane Mac adapter must use
the former.

Do not run this on an unattended host. The repository does not claim a physical
sleep/wake result until this probe has been run and its output has been retained
with the normal environment and source revision evidence.
