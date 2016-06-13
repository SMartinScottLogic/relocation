'use strict'

var filesize = require('filesize')
var ffi = require('ffi')
var ref = require('ref')
var StructType = require('ref-struct')

var fsblkcnt_t = 'ulong';
var fsfilcnt_t = 'ulong';

var StatVFS = StructType({
    f_bsize: 'ulong',
    f_frsize: 'ulong',
    f_blocks: fsblkcnt_t,
    f_bfree: fsblkcnt_t,
    f_bavail: fsblkcnt_t,
    f_files: fsfilcnt_t,
    f_ffree: fsfilcnt_t,
    f_favail: fsfilcnt_t,
    f_fsid: 'ulong',
    f_flag: 'ulong',
    f_namemax: 'ulong'
})

var StatVFSPtr = ref.refType(StatVFS)

var statvfs = ffi.Library(null, {
    'statvfs': ['int', ['string', StatVFSPtr]]
})

module.exports = exports = function statvfs_runner(path) {
    var vfs = new StatVFS()
    var r = statvfs.statvfs(path, vfs.ref())
    if (r !== 0) {
        return r
    }
    //console.log('statvfs', path, r, vfs)
    return {
        human: {
            block_size: filesize(vfs.f_bsize, { standard: "iec" }),
            fragment_size: filesize(vfs.f_frsize, { standard: "iec" }),
            size: filesize((vfs.f_blocks * vfs.f_frsize), { standard: "iec" }),
            free: filesize((vfs.f_bfree * vfs.f_bsize), { standard: "iec" }),
            available: filesize((vfs.f_bavail * vfs.f_bsize), { standard: "iec" })
        },
        machine: {
            block_size: vfs.f_bsize,
            fragment_size: vfs.f_frsize,
            size: (vfs.f_blocks * vfs.f_frsize),
            free: (vfs.f_bfree * vfs.f_bsize),
            available: (vfs.f_bavail * vfs.f_bsize)
        },
        raw: vfs
    }
}