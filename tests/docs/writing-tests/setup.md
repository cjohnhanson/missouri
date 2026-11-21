<!-- A suite in tests/missouri for the page's "Running tests" commands:
state-a, whose transition writes greeting.txt, and state-b, which holds
the file and asserts on it. -->

```console
$ missouri init -d tests/missouri
initialized missouri project at [CWD]/tests/missouri/.missouri

$ missouri state add state-a -d tests/missouri
created state "state-a"

$ missouri state add state-b -d tests/missouri
created state "state-b"

```

```yaml {filename="tests/missouri/state-a/.missouri/missouri.yml"}
transitions:
  - name: "write greeting"
    command: "echo hello > greeting.txt"
    target: ../state-b
```

```yaml {filename="tests/missouri/state-b/.missouri/missouri.yml"}
assertions:
  - name: "greeting says hello"
    command: "grep -q hello greeting.txt"
```

```text {filename="tests/missouri/state-b/greeting.txt"}
hello
```

