//! 验证命门：造一个 metadata parent=<文件夹uuid> 的 rmdoc upload，看 xochitl 是否归档进该文件夹。
use pkm_device::cardnote::pack_rmdoc_in;
use std::time::Duration;
use device_core::inject::upload_document;
fn main() {
    let parent = std::env::args().nth(1).expect("用法: verify_parent <folder-uuid>");
    let my_uuid = uuid::Uuid::new_v4().to_string();
    let rmdoc = pack_rmdoc_in(&my_uuid, "__parent验证__", &["parent 落文件夹验证页"], &parent).expect("pack");
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(20)).build();
    match upload_document(&agent, "10.11.99.1", &rmdoc, "__parent验证__.rmdoc", "application/zip") {
        Ok(r) => println!("upload ok, my_uuid={my_uuid} resp={r}"),
        Err(e) => println!("upload err: {e}"),
    }
}
