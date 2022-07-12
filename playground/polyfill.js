// @ts-check

const FILETYPE_UNKNOWN = 0;
const FILETYPE_DIRECTORY = 3;
const FILETYPE_REGULAR_FILE = 4;

const OFLAGS_CREAT = 0x1;
const OFLAGS_DIRECTORY = 0x2;
const OFLAGS_EXCL = 0x4;
const OFLAGS_TRUNC = 0x8;

class File {
  constructor(data) {
    this.data = new Uint8Array(data);
  }

  get size() {
    return this.data.byteLength;
  }

  open() {
    return new OpenFile(this);
  }

  stat() {
    return {
      dev: 0n,
      ino: 0n,
      file_type: this.file_type,
      nlink: 0n,
      size: BigInt(this.size),
      atim: 0n,
      mtim: 0n,
      ctim: 0n,
    };
  }

  truncate() {
    this.data = new Uint8Array([]);
  }
}

class OpenFile {
  file_type = FILETYPE_REGULAR_FILE;

  constructor(file) {
    this.file = file;
    this.file_pos = 0;
  }

  get size() {
    return this.file.size;
  }

  read(len) {
    if (this.file_pos < this.file.data.byteLength) {
      let slice = this.file.data.slice(this.file_pos, this.file_pos + len);
      this.file_pos += slice.length;
      return [slice, 0];
    } else {
      return [[], 0];
    }
  }

  write(buffer) {
    if (this.file_pos + buffer.byteLength > this.size) {
      let old = this.file.data;
      this.file.data = new Uint8Array(this.file_pos + buffer.byteLength);
      this.file.data.set(old);
    }
    this.file.data.set(
      buffer.slice(0, this.size - this.file_pos),
      this.file_pos
    );
    this.file_pos += buffer.byteLength;
    return 0;
  }

  stat() {
    return this.file.stat();
  }
}

class Directory {
  file_type = FILETYPE_DIRECTORY;

  constructor(contents) {
    this.directory = contents;
  }

  open() {
    return this;
  }

  get_entry_for_path(path) {
    let entry = this;
    for (let component of path.split('/')) {
      if (component == '') break;
      if (entry.directory[component] != undefined) {
        entry = entry.directory[component];
      } else {
        return null;
      }
    }
    return entry;
  }

  /**
   * @param {string} path
   */
  create_entry_for_path(path) {
    let entry = this;
    let components = path.split('/').filter((component) => component != '/');
    for (let i in components) {
      let component = components[i];
      if (entry.directory[component] != undefined) {
        entry = entry.directory[component];
      } else {
        if (i == components.length - 1) {
          entry.directory[component] = new File(new ArrayBuffer(0));
        } else {
          entry.directory[component] = new Directory({});
        }
        entry = entry.directory[component];
      }
    }
    return entry;
  }
}

class PreopenDirectory extends Directory {
  constructor(name, contents) {
    super(contents);
    this.prestat_name = new TextEncoder('utf-8').encode(name);
  }
}

class Stdio {
  file_type = FILETYPE_UNKNOWN;
  on_write = (data) => 0;
  on_read = (len) => [new Uint8Array(), 0];

  constructor({ read, write }) {
    if (read) {
      this.on_read = read;
    }

    if (write) {
      this.on_write = write;
    }
  }

  read(len) {
    return this.on_read(len);
  }

  write(buffer) {
    console.log(
      'RIQ2',
      'Class Stdio::write',
      [new TextDecoder('utf-8').decode(buffer)],
      [buffer]
    );
    return this.on_write(buffer);
  }
}

/**
 * @param {WebAssembly.Module} wasm
 * @param {string[]} args
 * @param {{ [key:string]: string }} env
 * @param {(Stdio|PreopenDirectory)[]} fds
 *
 * @returns {Promise<WebAssembly.Instance>}
 */
async function WASM_WASI_instantiate(wasm, args, env, fds) {
  const inst = await WebAssembly.instantiate(wasm, {
    wasi_snapshot_preview1: {
      proc_exit() {
        console.log('proc_exit');
      },
      random_get() {
        console.log('random_get');
      },
      args_sizes_get(argc, argv_buf_size) {
        let buffer = new DataView(inst.exports.memory.buffer);
        console.log('args_sizes_get(', argc, ', ', argv_buf_size, ')');
        buffer.setUint32(argc, args.length, true);
        let buf_size = 0;
        for (let arg of args) {
          buf_size += arg.length + 1;
        }
        buffer.setUint32(argv_buf_size, buf_size, true);
        console.log(
          buffer.getUint32(argc, true),
          buffer.getUint32(argv_buf_size, true)
        );
        return 0;
      },
      args_get(argv, argv_buf) {
        let buffer = new DataView(inst.exports.memory.buffer);
        let buffer8 = new Uint8Array(inst.exports.memory.buffer);
        console.log('args_get(', argv, ', ', argv_buf, ')');
        let orig_argv_buf = argv_buf;
        for (let i = 0; i < args.length; i++) {
          buffer.setUint32(argv, argv_buf, true);
          argv += 4;
          let arg = new TextEncoder('utf-8').encode(args[i]);
          buffer8.set(arg, argv_buf);
          buffer.setUint8(argv_buf + arg.length, 0);
          argv_buf += arg.length + 1;
        }
        console.log(
          new TextDecoder('utf-8').decode(
            buffer8.slice(orig_argv_buf, argv_buf)
          )
        );
        return 0;
      },

      environ_sizes_get(environ_count, environ_size) {
        let buffer = new DataView(inst.exports.memory.buffer);
        console.log(
          'environ_sizes_get(',
          environ_count,
          ', ',
          environ_size,
          ')'
        );
        buffer.setUint32(environ_count, env.length, true);
        let buf_size = 0;
        for (let environ of env) {
          buf_size += environ.length + 1;
        }
        buffer.setUint32(environ_size, buf_size, true);
        console.log(
          buffer.getUint32(environ_count, true),
          buffer.getUint32(environ_size, true)
        );
        return 0;
      },
      environ_get(environ, environ_buf) {
        let buffer = new DataView(inst.exports.memory.buffer);
        let buffer8 = new Uint8Array(inst.exports.memory.buffer);
        console.log('environ_get(', environ, ', ', environ_buf, ')');
        let orig_environ_buf = environ_buf;
        for (let i = 0; i < env.length; i++) {
          buffer.setUint32(environ, environ_buf, true);
          environ += 4;
          let e = new TextEncoder('utf-8').encode(env[i]);
          buffer8.set(e, environ_buf);
          buffer.setUint8(environ_buf + e.length, 0);
          environ_buf += e.length + 1;
        }
        console.log(
          new TextDecoder('utf-8').decode(
            buffer8.slice(orig_environ_buf, environ_buf)
          )
        );
        return 0;
      },

      clock_time_get(id, precision, time) {
        let buffer = new DataView(inst.exports.memory.buffer);
        //console.log("clock_time_get(", id, ", ", precision, ", ", time, ")");
        buffer.setBigUint64(time, 0n, true);
        return 0;
      },
      fd_close() {
        console.log('fd_close');
      },
      fd_filestat_get(fd, buf) {
        let buffer = new DataView(inst.exports.memory.buffer);
        console.warn('fd_filestat_get(', fd, ', ', buf, ')');
        if (fds[fd] != undefined) {
          let stat = fds[fd].stat();
          buffer.setBigUint64(buf, stat.dev, true);
          buffer.setBigUint64(buf + 8, stat.ino, true);
          buffer.setUint8(buf + 16, stat.file_type);
          buffer.setBigUint64(buf + 24, stat.nlink, true);
          buffer.setBigUint64(buf + 32, stat.size, true);
          buffer.setBigUint64(buf + 38, stat.atim, true);
          buffer.setBigUint64(buf + 46, stat.mtim, true);
          buffer.setBigUint64(buf + 52, stat.ctim, true);
          return 0;
        } else {
          return -1;
        }
      },
      fd_read(fd, iovs_ptr, iovs_len, nread_ptr) {
        let buffer = new DataView(inst.exports.memory.buffer);
        let buffer8 = new Uint8Array(inst.exports.memory.buffer);
        console.log(
          'fd_read(',
          fd,
          ', iovs_ptr',
          iovs_ptr,
          ', iovs_len',
          iovs_len,
          ', nread_ptr',
          nread_ptr,
          ')'
        );
        if (fds[fd] != undefined) {
          buffer.setUint32(nread_ptr, 0, true);
          for (let i = 0; i < iovs_len; i++) {
            let [ptr, len] = [
              buffer.getUint32(iovs_ptr + 8 * i, true),
              buffer.getUint32(iovs_ptr + 8 * i + 4, true),
            ];
            let [data, err] = fds[fd].read(len);
            console.log(`[fd_read] read(len) on fd ${fd} returned`, [
              data,
              err,
            ]);
            if (err != 0) {
              return err;
            }
            buffer8.set(data, ptr);
            buffer.setUint32(
              nread_ptr,
              buffer.getUint32(nread_ptr, true) + data.length,
              true
            );
          }
          return 0;
        } else {
          return -1;
        }
      },
      fd_readdir(fd, buf, buf_len, cookie, bufused) {
        let buffer = new DataView(inst.exports.memory.buffer);
        let buffer8 = new Uint8Array(inst.exports.memory.buffer);
        console.warn(
          'fd_readdir(',
          fd,
          ', ',
          buf,
          ', ',
          buf_len,
          ', ',
          cookie,
          ', ',
          bufused,
          ')'
        );
        // 8 ,  3408816 ,  128 ,  0n ,  1032332
        if (fds[fd] != undefined && fds[fd].directory != undefined) {
          buffer.setUint32(bufused, 0, true);

          console.log(
            cookie,
            Object.keys(fds[fd].directory).slice(Number(cookie))
          );
          if (cookie >= BigInt(Object.keys(fds[fd].directory).length)) {
            console.log('end of dir');
            return 0;
          }
          let next_cookie = cookie + 1n;
          for (let name of Object.keys(fds[fd].directory).slice(
            Number(cookie)
          )) {
            let entry = fds[fd].directory[name];
            console.log(name, entry);
            let encoded_name = new TextEncoder('utf-8').encode(name);

            let offset = 24 + encoded_name.length;

            if (buf_len - buffer.getUint32(bufused, true) < offset) {
              console.log('too small buf');
              break;
            } else {
              console.log('next_cookie =', next_cookie, buf);
              buffer.setBigUint64(buf, next_cookie, true);
              next_cookie += 1n;
              buffer.setBigUint64(buf + 8, 1n, true); // inode
              buffer.setUint32(buf + 16, encoded_name.length, true);
              buffer.setUint8(buf + 20, entry.file_type);
              buffer8.set(encoded_name, buf + 24);
              console.log('buffer =', buffer8.slice(buf, buf + offset));
              buf += offset;
              buffer.setUint32(
                bufused,
                buffer.getUint32(bufused, true) + offset,
                true
              );
              console.log();
            }
          }
          console.log('used =', buffer.getUint32(bufused, true));
          return 0;
        } else {
          return -1;
        }
      },
      fd_seek() {
        console.log('fd_seek');
      },
      fd_write(fd, iovs_ptr, iovs_len, nwritten_ptr) {
        console.log('I am in fd_write');
        let buffer = new DataView(inst.exports.memory.buffer);
        let buffer8 = new Uint8Array(inst.exports.memory.buffer);
        //console.log("fd_write(", fd, ", ", iovs_ptr, ", ", iovs_len, ", ", nwritten_ptr, ")");
        if (fds[fd] != undefined) {
          buffer.setUint32(nwritten_ptr, 0, true);
          for (let i = 0; i < iovs_len; i++) {
            let [ptr, len] = [
              buffer.getUint32(iovs_ptr + 8 * i, true),
              buffer.getUint32(iovs_ptr + 8 * i + 4, true),
            ];
            console.log(
              'GOING to write',
              ptr,
              len,
              buffer8.slice(ptr, ptr + len)
            );
            console.log('want to write', i, 'out of', iovs_len - 1);
            let err = fds[fd].write(buffer8.slice(ptr, ptr + len));
            console.log(
              'did write',
              i,
              'out of',
              iovs_len - 1,
              'got err?',
              err
            );

            if (err != 0) {
              return err;
            }
            buffer.setUint32(
              nwritten_ptr,
              buffer.getUint32(nwritten_ptr, true) + len,
              true
            );
          }
          return 0;
        } else {
          return -1;
        }
      },
      path_create_directory() {
        console.log('path_create_directory');
      },
      path_filestat_get(fd, flags, path_ptr, path_len, buf) {
        let buffer = new DataView(inst.exports.memory.buffer);
        let buffer8 = new Uint8Array(inst.exports.memory.buffer);
        console.warn(
          'path_filestat_get(',
          fd,
          ', ',
          flags,
          ', ',
          path_ptr,
          ', ',
          path_len,
          ', ',
          buf,
          ')'
        );
        if (fds[fd] != undefined && fds[fd].directory != undefined) {
          let path = new TextDecoder('utf-8').decode(
            buffer8.slice(path_ptr, path_ptr + path_len)
          );
          console.log('file =', path);
          let entry = fds[fd].get_entry_for_path(path);
          if (entry == null) {
            return -1;
          }
          // FIXME write filestat_t
          return 0;
        } else {
          return -1;
        }
      },
      path_link() {
        console.log('path_link');
      },
      path_open(
        fd,
        dirflags,
        path_ptr,
        path_len,
        oflags,
        fs_rights_base,
        fs_rights_inheriting,
        fdflags,
        opened_fd_ptr
      ) {
        let buffer = new DataView(inst.exports.memory.buffer);
        let buffer8 = new Uint8Array(inst.exports.memory.buffer);
        console.log(
          'path_open(',
          dirflags,
          ', ',
          path_ptr,
          ', ',
          path_len,
          ', ',
          oflags,
          ', ',
          fs_rights_base,
          ', ',
          fs_rights_inheriting,
          ', ',
          fdflags,
          ', ',
          opened_fd_ptr,
          ')'
        );
        if (fds[fd] != undefined && fds[fd].directory != undefined) {
          let path = new TextDecoder('utf-8').decode(
            buffer8.slice(path_ptr, path_ptr + path_len)
          );
          console.log(path);
          let entry = fds[fd].get_entry_for_path(path);
          if (entry == null) {
            if (oflags & (OFLAGS_CREAT == OFLAGS_CREAT)) {
              entry = fds[fd].create_entry_for_path(path);
            } else {
              return -1;
            }
          } else if (oflags & (OFLAGS_EXCL == OFLAGS_EXCL)) {
            return -1;
          }
          if (
            oflags & (OFLAGS_DIRECTORY == OFLAGS_DIRECTORY) &&
            fds[fd].file_type != FILETYPE_DIRECTORY
          ) {
            return -1;
          }
          if (oflags & (OFLAGS_TRUNC == OFLAGS_TRUNC)) {
            entry.truncate();
          }
          fds.push(entry.open());
          let opened_fd = fds.length - 1;
          buffer.setUint32(opened_fd_ptr, opened_fd, true);
        } else {
          return -1;
        }
      },
      path_readlink() {
        console.log('path_readlink');
      },
      path_remove_directory() {
        console.log('path_remove_directory');
      },
      path_rename() {
        console.log('path_rename');
      },
      path_unlink_file() {
        console.log('path_unlink_file');
      },
      sched_yield() {
        console.log('sched_yield');
      },
      fd_prestat_get(fd, buf_ptr) {
        let buffer = new DataView(inst.exports.memory.buffer);
        console.log('fd_prestat_get(', fd, ', ', buf_ptr, ')');
        if (fds[fd] != undefined && fds[fd].prestat_name != undefined) {
          const PREOPEN_TYPE_DIR = 0;
          buffer.setUint32(buf_ptr, PREOPEN_TYPE_DIR, true);
          buffer.setUint32(buf_ptr + 4, fds[fd].prestat_name.length);
          return 0;
        } else {
          return -1;
        }
      },
      fd_prestat_dir_name(fd, path_ptr, path_len) {
        console.log(
          'fd_prestat_dir_name(',
          fd,
          ', ',
          path_ptr,
          ', ',
          path_len,
          ')'
        );
        if (fds[fd] != undefined && fds[fd].prestat_name != undefined) {
          let buffer8 = new Uint8Array(inst.exports.memory.buffer);
          buffer8.set(fds[fd].prestat_name, path_ptr);
          return 0;
        } else {
          return -1;
        }
      },
    },
  });

  return inst;
}
