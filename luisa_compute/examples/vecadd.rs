use luisa::prelude::*;
use luisa_compute as luisa;
use std::env::current_exe;

fn main() {
    luisa::init_logger_verbose();
    let args: Vec<String> = std::env::args().collect();
    assert!(
        args.len() <= 2,
        "Usage: {} <backend>. <backend>: cpu, cuda, dx, metal, remote",
        args[0]
    );

    let ctx = Context::new(current_exe().unwrap());
    let device = ctx.create_device(if args.len() == 2 {
        args[1].as_str()
    } else {
        "cpu"
    });
    let x = device.create_buffer::<f32>(1024);
    let y = device.create_buffer::<f32>(1024);
    let z = device.create_buffer::<f32>(1024);
    x.view(..).fill_fn(|i| i as f32);
    y.view(..).fill_fn(|i| 1000.0 * i as f32);

    let kernel = device.create_kernel_with_options::<fn(Buffer<f32>)>(
        KernelBuildOptions {
            time_trace: true,
            name: Some("vecadd".into()),
            ..Default::default()
        },
        &track!(|buf_z| {
            // z is pass by arg
            let buf_x = &x; // x and y are captured
            let buf_y = &y;
            let tid = dispatch_id().x;
            let x = buf_x.read(tid);
            let y = buf_y.read(tid);
            let vx = 2.0_f32.var(); // create a local mutable variable
            *vx += x; // store to vx
            buf_z.write(tid, vx + y);
        }),
    );
    kernel.dispatch([1024, 1, 1], &z);
    let z_data = z.view(..).copy_to_vec();
    println!("{:?}", &z_data[0..16]);
    {
        let s = device.default_stream();
        let t = std::time::Instant::now();
        let times = 1000;
        let s = s.scope();
        for _ in 0..times {
           
            s.submit_with_callback([], ||{
                std::hint::black_box(());
            });
            s.synchronize();
        }
        let elapsed = t.elapsed().as_micros();
        println!("{} us", elapsed as f32 / times as f32);
    }
}
